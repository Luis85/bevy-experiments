# Current compatibility contract

Use this page for the current SDK handoff. Versioned reviews, capture records and
the changelog describe their named releases and retain the values tested then.
`python3 scripts/check_compatibility.py` checks this table, the current networking
guides and all workspace package versions against the source constants.

<!-- compatibility:generated:start -->
| Contract | Current value |
|---|---|
| Release | `0.24.0` |
| Wire protocol | `11` |
| TLS ALPN | `terra-world/11` |
| Domain save | `1` |
| Regional database schema | `1` |
| Minimum Rust | `1.97.1` |
| Acceptance toolchain | `1.99.1` |
<!-- compatibility:generated:end -->

## Executables and transport

Release 0.24 adds useful public-offer lifecycle and native operational intent
without changing a command's serialized shape. Wire 11 and its ALPN remain. Matching wire numbers do not establish arbitrary mixed-version
qualification; the delivered release is tested with its matching components.

Upgrade clients and hosts together. Protocol 11 adds `DissolveFaction` for a
sole-leader community without shared land or settlement territory. Protocol 10
introduced explicit membership, invitation and leadership commands. Protocol 9 introduced the
recipient-private company cash report and accounting-capacity hold. Earlier
peers reject during admission; a matching protocol number alone does not
certify arbitrary mixed executable versions. TLS derives its ALPN identifier
from the protocol constant and does not fall back to plaintext.

## Saves, sources and content

No schema migration is required for this pass. Compatible complete older saves
retain their positions, economy, ownership and source identity; additive defaults
from earlier releases still apply. Source and catalog identity checks remain
mandatory. A save version of 1 does not grant permission to swap terrain,
semantic features or catalogs underneath a world. There is no automatic
geometry/source migration tool.

Community membership changes use the existing faction representation. Leaving
or removing a member retains faction property and public funds; leadership must
be transferred before the current leader can leave. Presentation packs remain
optional immutable client content and do not alter saved authority.

## Presentation and recovery formats

Asset-pack schema **1** remains, but the new `vehicle_model` binding target and
optional `animation.vehicle_motion` policy require **Terra 0.15 or later**.
Older readers reject these unsupported inputs. Existing kind/species packs
retain their meaning and normalized identities; an absent wheel policy adds no
serialized field. Use the [asset-pack contract](ASSET_PACKS.md) and
[authoring guide](ASSET_AUTHORING.md) when selecting a reader for a new pack.
Pack compatibility is separate from domain-save and network compatibility.

Optional `animation.human` material/pose policy and model-specific `driver_seat`
require **Terra 0.16 or later**. They remain schema-1 extensions; an older reader
rejects those new fields. Existing packs that omit both fields retain their
normalized identities; an absent optional field adds no serialized value.
Character customization reads the existing saved profile and
adds no new authoritative appearance or occupancy state.

The native backup browser introduced in 0.15 expects the complete managed
wrapper: `world-backup.json` beside `bundle/`. That schema-1 launcher record
retains character and presentation settings plus the expected bundle identity;
it is separate from the regional database schema and authoritative save.
A bare operator recovery bundle has no launcher record and uses the existing
command-line workflow. See [Recovery bundles](RECOVERY_BUNDLES.md) for supported
worlds and the distinction between exact restore and content migration.

Recovery-copy naming and Rename use the existing schema-1 profile title field.
They do not migrate gameplay, modify backup metadata or reinterpret a duplicate
title as world identity. Live commitments and selected guidance are read-only
or session UI state, with no additional saved progression ledger.

## File publication and SDK error compatibility

The 0.24 `SupplyContractBlocker::ObsoleteOffer` variant is a public API addition:
downstream exhaustive matches must handle it. Supply demand/lifecycle is derived
from existing state, so no saved fields or migration are introduced. Accepted
terms stay unchanged; public retirement requires exact original-issuer refund.
Portable protected-output comparison now uses exact `same-file = 1.0.6`, already
present in the locked external package set. All external package versions remain.

The additive 0.23 `SupplyContractAction`, `SupplyContractBlocker` and
`SandboxState::supply_contract_readiness` API expose current supply admission.
They do not reserve stock/capacity or serialize a new world field. Commands use
the same preparation again against current authority. See
[settlements and economy](SETTLEMENTS_AND_ECONOMY.md) for replica scope and recovery.

Terra 0.22 shares private file publication through
`terra_sandbox::publication`. Domain/native snapshots, network host saves and
resume identities, regional backup/archive files, managed metadata and retained
diagnostic reports use this contract. It does not change their serialized
schemas or replace SQLite's transaction and generation checks.

The publisher flushes complete temporary-file bytes before replacement or
exclusive creation. On Unix it checks the containing-directory synchronization;
newly created save/recovery parents and their ancestor entries are synchronized
before payload publication. Relative filenames use `.` as their containing
directory. New private files use mode 0600 and new private directories use 0700;
existing directories retain their permissions. These Unix modes do not qualify
Windows ACLs. On non-Unix systems the contract checks file synchronization and
namespace publication without claiming an equivalent directory-fsync operation
or tested physical power-loss survival.

| Result | Caller contract |
|---|---|
| Success | Required file and supported namespace synchronization calls succeeded. Physical storage qualification remains separate. |
| `PublicationError::NotPublished` | No final file was published by this attempt; any prior destination remains. Cleanup owns only staging this operation created. |
| `PublicationError::Published` | The new complete file is visible, but later synchronization or cleanup failed. Retain and verify it. Do not claim rollback, successful saving or an untouched prior destination. |

`SandboxError`, `StoreError` and `NetError` gain a transparent `Publication`
variant. Downstream exhaustive matches need to handle it; `published()` supplies
the stage without parsing error text. Existing write entry points remain.
`LocalWorldSave::write_with_sync` and `SessionIdentity::save_with_sync` add an
explicit host synchronization boundary. The public recovery convenience
functions retain `Result<_, String>` signatures; additive
`create_recovery_bundle_with_sync` and `restore_recovery_bundle_with_sync`
return `RecoveryPublicationError`, whose `published()` describes the final
completion-marker stage. Custom callbacks must honor the host's documented
synchronization contract; they are not permission to acknowledge skipped work.

Recovery flushes its bounded payload tree before publishing the final index or
restore receipt. A completed backend remains if a later launcher record/profile
cannot be confirmed, and the error distinguishes that retained backend from a
completed native wrapper. An intermediate database alone does not make an
incomplete outer recovery tree complete. A published final marker is retained
after uncertainty; the operation does not issue a successful completion receipt.

Native final JSON saving reports an error exit on uncertain publication and
keeps its dirty state. An uncertain private multiplayer identity write prevents
connected-state admission, while retaining the visible identity for validated
resume. Error text does not expose resume capabilities. See
[file-publication outcomes](OPERATIONS.md#file-publication-outcomes) and
[recovery bundles](RECOVERY_BUNDLES.md) for verification and recovery guidance.

## Managed launch and checkpoint compatibility

Terra 0.21 adds a private schema-1 `world-started.json` beside a managed
`world.json`. This is a launcher admission record, separate from domain save 1
and regional schema 1. It records that initialization was admitted, **not** that
a checkpoint succeeded. Its immutable binding covers the profile location,
progress path and storage mode, and world/source identity. Rename and ordinary
presentation/clock settings retain the same binding. Direct-play worlds without
`--world-profile` do not use this managed-launch protocol.

Only the process that exclusively publishes a new witness may initialize absent
progress. Publication flushes its file bytes and, on Unix, synchronizes the
parent directory before admitting initialization. Later launches require a
validated existing checkpoint. Missing, empty, corrupt or generation-zero
regional progress is rejected before normal writable startup can initialize a
store. The JSON path likewise requires a valid complete snapshot. An interrupted
first launch without a successful save therefore fails closed on reopening.
Keep the record and use a verified backup or create a separate new world;
deleting the record to retry would remove the evidence this protocol preserves.

Valid existing legacy progress is checked before it gains a witness. A legacy
profile with `checkpoint_present=false`, no witness and already-lost progress is
indistinguishable from an unstarted profile; this release cannot recover that
missing historical evidence. Older launchers do not enforce the witness, even
though the authoritative save schemas are unchanged. Using one to reopen a
managed world does not retain the 0.21 missing-progress protection.

Terra 0.22 additionally holds an OS-backed, nonblocking exclusive lease on
`world-session.lock` beside the managed profile. A participating runtime acquires
it before initialization and retains its file handle through game shutdown and
final profile/settings publication, then releases it before returning to the
launcher. A second participating process opening the same canonical managed
profile is rejected promptly with an already-open explanation. Different managed
worlds have independent leases.

The lock is a stable private marker with a checked format, not a PID file or a
checkpoint witness. Its pathname is never unlinked to release ownership. Closing
the owning handle or terminating its process releases the OS lock; a valid
unlocked marker is reusable. Acquisition rechecks its containing-directory
synchronization, including after a previous uncertain marker creation. A
malformed or aliased marker is rejected rather than silently replaced. Keep the
separate initialization witness: an interrupted first launch without a valid
checkpoint still fails closed after the lease is released.

This is cooperative local managed-world ownership. Older launchers, direct-play
or independent SDK writers, administrative path replacement and network
filesystems are outside that guarantee. The lease does not replace regional
optimistic-generation checks or establish distributed authority. The Rust lock
API has a Windows mapping, but Windows contention/crash behavior and the complete
native journey require their own executed validation.

Retain the complete managed directory when making a raw archival copy after
cleanly closing its processes: profile, witness, session-lock marker if present,
progress and any SQLite sidecar files belong together. The witness binds original
paths, so this copy is not a relocation procedure. Use the supported verified
recovery workflow to create a new managed location; its own profile gains a
witness on its first admitted launch. Retain the complete backup wrapper described above. Source/content
dependencies still need the policy in
[Desktop delivery](DESKTOP_DELIVERY.md#upgrade-and-recovery).

Native output preflight and runtime writers reserve the managed profile,
initialization witness and session-lease paths, including output-parent creation
and existing filesystem aliases. Screenshots, political exports, backup outputs
and diagnostic reports/locks cannot occupy those paths. A new witness whose
publication is uncertain remains visible, and that attempt refuses initialization
without claiming rollback. General file-publication stage handling now follows
the shared 0.22 contract above; target-filesystem power-loss qualification remains
a separate open requirement.

## Fleet navigation

The native Fleet inspector has six sections: **Company, Finances, Drivers,
Routes, Recovery and Depot**. At small window sizes or enlarged text, the
section chooser retains the same destinations in a scrolling inspector.

## Qualification

The [current validation record](VALIDATION.md) identifies exactly what ran.
The [desktop delivery guide](DESKTOP_DELIVERY.md) separates reproducible
packaging and CI configuration from executed operating-system and physical
device qualification. A source ZIP or CI workflow is not evidence of a native
Windows playtest.
