# Recover a local world with its terrain and configuration

The reference local host can now create a **self-contained private recovery
directory**. It contains the last committed regional database, the exact pinned
catalog, a portable resolved `WorldConfig`, and all referenced prepared terrain
files. The optional local political GeoJSON input is retained byte for byte.
A procedural world records its supported generator revision instead.

This closes a practical recovery failure: a valid database alone cannot reopen a
prepared world after its original source directory or configuration is lost. The
new workflow validates those dependencies together and restores them to a new
location. It never rewrites an incident database or silently substitutes a
different map.

## Supported worlds

The current workflow supports the **reference local regional host**, the
unchanged built-in catalog, and either `terra-procedural-v2` or a validated
prepared heightmap/connected heightmap collection. It validates the complete
domain and the actual local host metadata, including the clock, local principal
and source identity. Dormant records, balances, inventories, communities,
ownership, transport obligations and the committed generation remain exact.

One bounded local political GeoJSON dependency is supported, including the
Natural Earth input used by ordinary menu worlds. Its exact source bytes and
dataset identity are retained. Player-authored political changes remain in the
saved host metadata and are restored separately from that immutable base input.
Both must match the resolved configuration.

Live sources, historical packs, custom catalogs and server databases are rejected
by this workflow. The handoff for unsupported cases is explicit:

| World or host | What must be retained and what remains unsupported |
| --- | --- |
| Managed portable JSON | Retain the complete managed folder, including saved JSON, `world.json`, `world-started.json` and the stable `world-session.lock` when present, plus the exact catalog, resolved configuration, external source/political dependencies and matching application release. The witness binds the original paths; a raw archival copy is not a supported relocation procedure. The menu does not create or validate a recovery package for this mode. A profile alone cannot recover gameplay progress. |
| Historical or live source | Retain the exact source revisions, original source/configuration files and any host-specific metadata alongside the checkpoint. Neither the menu nor the bundle CLI offers complete dependency recovery for these sources. A database export alone cannot reconstruct a missing historical pack or lost live-source revision. |
| Custom catalog | Retain the exact catalog, resolved host configuration, source dependencies and host metadata with the checkpoint. This reference bundle cannot validate or reconstruct that custom host contract. |
| Server database | [Operations](OPERATIONS.md) describes database backup/export and host-validated restore tools. Operators must separately preserve the deployment's configuration, source/catalog dependencies and required private host state. Those tools do not by themselves reconstruct complete source dependencies, resume capabilities, deployment TLS keys or public account policy. |

These retention requirements describe the inputs an operator must preserve;
they do not claim a verified end-to-end restoration procedure for unsupported
worlds. Keep complete original inputs until a host-specific recovery policy has
been implemented and rehearsed.

Presentation packs are optional client content and are not authoritative world
dependencies. They are not included here. Keep required presentation packs with
the matching application release, or use the procedural presentation option.

## Make and recover a backup from the native menu

**Your worlds → Make backup** captures the selected world's last committed
regional checkpoint using its actual resolved profile. Save and return to the
menu first. A world open in another participating session, or already undergoing
maintenance, must finish before native backup can proceed. The attempt fails
before creating any backup folder. Unsaved, portable-JSON,
historical and live-source worlds explain why this local recovery workflow is
unavailable. The ordinary backup action needs no configuration extraction.

One background operation copies and validates the world and its dependencies.
The menu remains responsive and displays the named operation; a second recovery,
world launch or application close waits for its result. A successful copy shows
the world name and checkpoint. Existing worlds and older backups remain intact.

**Your worlds → Backups and recovery** lists up to 24 recent completed copies,
inspecting at most 256 folders. The latest completed backup and recovered world
stay at the front of their respective lists for the current menu session,
including after navigation or refresh, so older future-dated records cannot
hide a successful result. Choose **Verify** to check the full bundle and
open a review naming its world, checkpoint, saved character, graphics settings,
backup time and exact folder. Set **Recovered world name** to a recognizable
name for this copy; the original backup title stays intact. **Back** cancels
that review. **Recover as new
world** creates a separate managed world with the recovered character and
presentation settings; it appears in **Your worlds** after the complete restore
succeeds. It does not launch automatically. Its profile records that a checkpoint
exists, and every runtime database, source, political, cache and manual-save path
is rehomed into its new world folder. Managed profiles treat an absent political
source as explicit, so a politics-free copy does not acquire the demo's default
dataset when launched.

**Your worlds → Rename** changes one selected world's menu name. It displays the
current character, source and creation time, accepts 1–48 characters, and returns
to the named entry after **Save name**. The list also shows creation times with
seconds in UTC. Names and dates are descriptive; distinct folders remain the
world identities, including when two copies have the same name or timestamp.
Use different names when retaining several copies of a checkpoint. Names survive
refresh, navigation and later launches without a new save format.

Renaming changes only the title in `world.json`. The full selected profile is
rechecked while holding the world's cooperative maintenance lease before atomic
replacement; a busy world, stale selection, changed profile, path alias or unmanaged
destination is rejected. The database, sources, character, presentation settings,
creation/last-played times and retained backups are unchanged. The name editor
cannot launch a world or mark an unplayed world as saved. Cancellation performs
no write. The latest renamed entry receives the same bounded session pin as a
completed recovery, so its Open action stays reachable.

Rename and native backup retain the same stable `world-session.lock` ownership
used by managed play through their final publication. A first maintenance
operation may create that coordination marker; it does not create
`world-started.json`, admit a first launch or initialize progress. Closing or
crashing the holder releases the OS lock, while the marker remains. Keep it in
place. This protects cooperating current launchers, not older executables,
direct SDK writers or arbitrary filesystem replacement. Native backup's lease
also keeps the profile and committed checkpoint together for its wrapper;
the backend operator workflow below remains an online database snapshot API.

The optional **Backup folder from another location** field accepts the entire
folder created by **Make backup**. It applies the same validation and review;
inspection does not copy, import, restore or change the selected folder. A bare
operator bundle has no launcher character/presentation record and continues to
use the command-line workflow below.

Managed copies live under `<user-data>/backups/backup-…`. Each private wrapper
contains exactly `world-backup.json` and `bundle/`. The bounded 8 KiB launcher
record retains the character/presentation, original world name, backup time,
checkpoint generation and expected bundle identity. It publishes last, outside
the immutable backend bundle. The native review retains its canonical path and
complete record bytes; changing metadata, list selection or checkpoint requires
a fresh review. The checkpoint named by the record must equal the verified
bundle's generation. Invalid character/name metadata is rejected before a new
world is allocated. Hashes and retained metadata detect changes within this
workflow; they do not authenticate a publisher.

Restore uses a newly claimed managed folder and publishes `world.json` only after
the backend restore and profile validation complete. Before a final marker is
published, a failed builder can clean its owned incomplete output. Once a
backend bundle, restore receipt, wrapper record or managed profile has been
published, a later synchronization failure retains that published output and
reports that completion was **not confirmed**. A complete backend bundle without
its launcher wrapper remains available to the operator verification workflow;
it is not advertised as a completed native backup. An incomplete directory
without a valid final record/profile is not offered as a resumable world.
An unexpected process termination can also leave incomplete output; existing
backups are never automatically deleted. Keep source/configuration files, retained wrappers and
filesystem ancestors unchanged during an operation, as with the backend tools.
Keep an independent protected copy for device-loss recovery; the menu provides
local recovery, not offsite transfer, encryption, retention policy or a restore
time promise.

The regression
`launcher::recovery::tests::managed_recovery_journey_preserves_complete_world_after_original_loss`
emits a bounded `LAUNCHER_RECOVERY_REPORT` under `--nocapture`. It exercises the
managed profile, complete backup, original input loss, verified new-world
restore, exact saved archive and presentation, discovery and repeat backup.
Separate tests cover stale confirmations, invalid/tampered metadata, incomplete
copies, protected output paths, single-worker admission and navigation while a
verification completes. Native interaction evidence is recorded separately.

## Create a recovery directory with the operator tool

Build the headless tool:

```sh
cargo build -p terra-bevy --no-default-features --bin terra-worldctl --locked
```

For this command-line workflow, use the **resolved configuration of the saved world**. A launcher profile
contains this as its `config` object; save that object as a JSON file. The entire
profile is a wrapper and is not a `WorldConfig`. A starter configuration that
still carries the default source revision cannot identify an existing prepared
world. Do not edit identity fields to bypass a mismatch.

```sh
target/debug/terra-worldctl bundle-create \
  saves/valley.db saved-world-config.json backups/valley-001
```

The parent directory must already exist. The destination must not exist, even as
an empty or incomplete directory. The tool uses SQLite's bounded online backup
API and validates the **copied snapshot**. Committed WAL data is included; later
in-memory changes are not. If the latest play must be included, request a save
and wait for its committed generation before creating the bundle.

The JSON receipt gives the `bundle_identity`, committed generation, region and
record counts, byte count and successful validation flags. Retain that identity
separately with the checkpoint's operational record. It is a SHA-256 digest of
the complete canonical index; that index binds every included file.

The final `recovery-bundle.json` is the publication marker. Files are copied,
checked and flushed, and their directories synchronized on Unix, before it
appears. The marker itself uses private no-clobber publication with a checked
parent-directory flush. A failure before that marker is published can remove
the owned incomplete output. A failure after publication keeps the complete
visible tree and reports uncertainty; it does not issue a successful receipt or
claim rollback. Verify retained output before using it, and preserve the original
backup/source until a recovery succeeds. An incomplete directory without the
marker is never admitted.

The SDK's `create_recovery_bundle_with_sync` and
`restore_recovery_bundle_with_sync` return `RecoveryPublicationError` with a
structural `published()` flag for containing workflows. A published backend is
not the same as a completed containing launcher wrapper. Existing convenience
functions retain their string-error signatures. Their host callback names the
actual directory-flush boundary; a custom callback must honor its documented
filesystem contract. On non-Unix platforms file bytes are flushed, while Unix
directory-fsync behavior and physical power-loss survival remain unqualified.
Normal commits may continue in the source database during SQLite's online
backup. Keep source/configuration files, retained bundles, output trees and
filesystem path components unchanged during maintenance. The tool does not
defend against another administrator concurrently replacing those paths.

## Verify a retained copy

```sh
target/debug/terra-worldctl bundle-verify backups/valley-001 \
  --expect-bundle sha256:YOUR_RETAINED_DIGEST
```

Replace `sha256:YOUR_RETAINED_DIGEST` with the exact receipt value. The option may
be omitted for an initial inspection, but an independently retained expected
identity detects replacement of both an index and its contents. Hashes are not
publisher authentication, encryption or a substitute for protecting the receipt.

Verification checks:

1. The bounded, canonical index and exact member inventory. Unindexed files,
   duplicate paths, nonportable names, symbolic links, special files, escaping
   paths and Unix hard-linked members are rejected.
2. Every file's size and SHA-256 digest, plus the prepared package's own source
   identity, file digests, provenance, seam and coverage validation. The political
   input, if present, must parse and match its configured and saved dataset ID.
3. SQLite integrity, checkpoint payload digests, complete domain references,
   catalog equality, source generation and local host metadata.
4. The complete committed archive digest, including all cold state and local
   metadata. The portable configuration may point only to the defined paths
   inside the recovery directory.

The database is bounded to 256 MiB, the config/index to 1 MiB each, and the
catalog copy and optional political input to 32 MiB each. A set admits at most
104 payload files and 600 MiB of
payload. Prepared-source importer limits remain in force, including its 256 MiB
aggregate content budget. These are admission ceilings, not measured capacity
or recovery-time promises.

Unix directories use mode `0700` and files `0600`; verification rejects broader
permissions. On Windows, protect the parent directory with an appropriate
operator-only ACL. This increment does not configure or qualify Windows ACLs.
Bundles contain private world data. Do not attach them to public bug reports or
include them in source/release archives. Receipts contain counts and hashes,
not complete world records.

## Restore without touching the incident world

```sh
target/debug/terra-worldctl bundle-restore \
  backups/valley-001 recovered/valley-001 \
  --expect-bundle sha256:YOUR_RETAINED_DIGEST

target/debug/terra-demo --config recovered/valley-001/world-config.json --no-default-politics
```

The destination parent must exist and the destination itself must be new.
Restore reads only the accepted bundle. It copies the committed database through
the SQLite backup API, installs the verified source package, checks the complete
archive again, and writes a `restore-receipt.json` only after success. The original
bundle's bytes remain unchanged.

The restored configuration contains absolute paths to its new database, terrain,
political input when present, cache and manual-save location. Starting the game from another working directory
does not reconnect it to the old world or source tree. The configured runtime
must support the recorded catalog and source revision. Launch a copy with the
matching application release; a different executable is not qualified merely
because the directory restored successfully. `--no-default-politics` prevents
the reference app from adding an implicit political dataset to a deliberately
politics-free restored world. An explicit bundled political input remains
honored, so the same command supports either case.

The restored database preserves the committed generation. It is a new physical
authority location, not a second live writer for the same deployed world. Stop
or isolate the old authority before promoting a recovered copy. Keep the incident
files unchanged for diagnosis.

The restored directory is a **working world**, not another immutable bundle:
normal saves change its database. Keep the original bundle separately. Create a
new bundle when another recovery point is required.

## Rehearse and record the boundary

```sh
cargo test -p terra-bevy --no-default-features recovery::tests --locked
cargo run -p terra-bevy --no-default-features \
  --example recovery_bundle_probe --locked -- --output recovery-bundle-report.json
```

The probe creates an original local prepared world with 1,024 geographically
separated resource groups, two principals, owned living bodies, a faction and
shared property. It makes two checkpoints, captures the complete state, and
uses the unchanged menu political input plus one saved authored territory, and
creates a dependency-complete bundle. It then removes the original database,
configuration and terrain/political source directory before restoring elsewhere. The report checks
complete state, host metadata, source/catalog identity, generation, cold records,
exact political source bytes and authored political state, private permissions
where supported, retained bundle bytes and rejection of a
damaged copy. It prints counts, hashes and measured maintenance durations only.

This is a bounded correctness rehearsal using authored resources and the original
prepared-source fixture. It does not assert terrain coverage for every distant
resource, connected-client throughput, renderer performance, a physical power
cut, offsite recovery or a promised restore time. The restored source is opened
headlessly; actual native startup is a separate acceptance check.

For deployment, record the recovery owner, chosen checkpoint cadence, retained
versions, retention expiry, access policy, independent offsite copy and its
verification result. Define acceptable data loss (RPO) and elapsed service
recovery (RTO), including detection, transfer, matching application/source
availability, operator actions and reopening the world. Rehearse those steps in
the intended environment. This increment provides the verifiable local unit;
it does not invent those operational decisions or delete backups automatically.

See also [Regional persistence](REGIONAL_PERSISTENCE.md),
[Operations](OPERATIONS.md), [Source distribution](SOURCE_BUNDLES.md) and
[Compatibility](COMPATIBILITY.md).
