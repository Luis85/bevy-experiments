# terra-persistence

Durable regional mutable-world storage for a single Rust authority. The crate
has no Bevy dependency. It uses bundled SQLite, WAL journaling with FULL
synchronization, checksummed payloads and optimistic generation checks.

`RegionStore` persists the shared gameplay ledger, geographic record pages and
game-owned metadata in one transaction. It materializes dependency-closed
working sets and preserves dormant records during mutation, movement, deletion,
checkpoint, restart and complete-world export. The domain's existing aggregate
limits remain in force; dormant records reserve their capacity.

`RegionWorker` serializes background hydration and checkpoint work with one
outstanding snapshot, bounded queues, nonblocking polling and explicit final
drain. The host retains its visible state and suspends authoritative mutation
until completion. Immutable catalog-pinned partition proofs share their storage
across clones. Checkpoints certify unchanged dependency-closed resource/source
records, validate actual cold bodies/housing/ecology against current global
ledgers, and reuse exactly equal checksummed page bytes. Untrusted decode still
receives complete canonical validation; schema, source/catalog, generation and
external-write epoch guards remain enforced. Work counters and separate
compose/select/encode/transaction/install timings expose the optimization's scope.
Disconnected owners’ bodies and homes can leave the working set without losing
account/vehicle/housing references; reconnect must hydrate before reactivation.

Schema 1 admission checks the supported columns, declared storage types,
nullability and primary-key ordering structurally, including otherwise hidden
generated columns. Index metadata must describe only the existing regional
primary key and the world's integer rowid key; extra UNIQUE autoindexes are
rejected, including constraints that could silently replace another region.
Foreign keys are unsupported because they can add mutation semantics outside
the checkpoint transaction's intended writes. SQL spelling,
whitespace and default-expression formatting are not compared. The existing
schema-1 databases and backup format are unchanged; hand-rebuilt incompatible
tables require explicit recovery or migration. Decoding also rejects duplicate
regional identities instead of collapsing them into one page.

See [the lifecycle and operations guide](../../docs/REGIONAL_PERSISTENCE.md) for
the exact active/dormant policy, APIs, native/server integration, backup and
recovery, migration boundaries and remaining scaling work.
The [operations CLI guide](../../docs/OPERATIONS.md) covers no-overwrite backup,
private portable archives and validated local/server JSON migration.

```sh
cargo test -p terra-persistence
cargo run -p terra-persistence --example regional_probe -- --output regional-report.json
```

The finite probe uses synthetic records in a temporary database. It checks exact
roundtrips and backup/restart, reports the actual active working set and measures
synchronous maintenance cost, background worker latency, snapshot submission and
nonblocking poll cost separately. It does not use network clients, real-world data or
a renderer, and does not establish production player capacity.


## Verified recovery

`RegionStore::open_read_only` supports maintenance inspection and export without
changing a source database's journal mode. `backup_to` now verifies a private
copied snapshot and atomically publishes a new output; incomplete/invalid copies
never appear at the final destination. `backup_with_options` adds a bounded copy
retry deadline and an exact-generation receipt. `terra-worldctl recover` uses
that path to restore a verified backup into a new database while preserving
source files, complete dormant state and private metadata.

Run the reproducible regional recovery workload:

```sh
cargo run -p terra-persistence --example recovery_probe --locked
```

It exercises 1,024+ occupied regions and 6,144 resource nodes with only one
requested active region, exact restore, corrupt-source refusal and unchanged
backup bytes. It is a finite storage workload; no connected players or renderer
are measured. See [operations](../../docs/OPERATIONS.md) for copy deadlines,
durability semantics and the matching host-configuration requirement.

Hydration remains a committed-state integrity barrier even when the caller
already holds a fully materialized world. Schema, descriptor, generation and
payload admission are rechecked through the guarded read cache before host
registration/edits. Unchanged snapshots avoid payload re-decoding; external
writes require fresh complete validation. The barrier never replaces valid
uncommitted live edits with the stored snapshot.
