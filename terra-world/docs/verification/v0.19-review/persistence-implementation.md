# Persistence integrity implementation — 0.19 candidate

Owner: dedicated persistence/recovery reviewer and implementer.

## Delivered

- `validate_schema` now uses `PRAGMA table_xinfo` to validate the existing
  schema-1 column contract: names/order, declared storage types (case-insensitive,
  trimmed), nullability, primary-key ordinals, and absence of hidden/generated
  columns. Both tables reject foreign keys, which can introduce unexpected
  cascade behavior. Existing object, descriptor, checksum and domain checks stay.
- Exact CREATE TABLE SQL and default expressions are not compared. No schema,
  save, wire, catalog or asset version changes are introduced.
- The complete regional decoder rejects duplicate keys before an earlier page
  can be silently accepted as overwritten authoritative state.
- The crate README records this supported contract.

## Focused regression coverage added

`altered_regional_schema_is_rejected_by_readers_and_live_checkpoint_barriers`
checks six same-name rewrites: missing/reordered composite keys, nullable
payloads, changed coordinate type, a generated column hidden from table_info,
and an added cascading foreign key. Writable/read-only/existing opens,
inspection/archive/restore, and a checkpoint on the already-open authority all
reject; active state, acknowledged and stored generation, and retained row count
remain unchanged.

`duplicate_regional_rows_cannot_be_collapsed_into_an_accepted_checkpoint`
reconstructs the original defect: remove the key, duplicate a valid checksummed
regional row, and adjust the declared payload bytes. Reads and writes reject
without acknowledging or installing a new checkpoint.

`supported_schema_one_remains_readable_and_writable_after_backup` validates the
positive compatibility route: original schema-1 store → online backup →
read-only archive → writable reopen → exact restore → next checkpoint, while the
source generation and complete state remain intact.

## Verification status and commands

Source files were formatted with the pinned Rust formatter. Compilation and test
execution are centrally coordinated by the integrator; this note does not claim
those commands passed before their recorded execution.

```sh
cargo test --locked -p terra-persistence
cargo test --locked -p terra-bevy --lib recovery
```

Use the integrator's pinned toolchain, shared cache and target directory.
Remaining G15/G16 storage-tail measurement and deployment recovery policy are
unchanged. This patch establishes storage-contract admission; it does not claim
physical power-loss or Windows device qualification.

## Cross-review polishing

The independent performance/content owner found an additional integrity path:
`UNIQUE(level) ON CONFLICT REPLACE` preserves column/PK metadata yet can silently
delete the preceding regional insert during an initial checkpoint. Structural
admission now also checks `pragma_index_list` and `pragma_index_xinfo`: the world
must use its existing integer rowid key without indexes, and regions must have
only the complete nonpartial unique primary-key index over level/x/y. Extra
SQLite-owned UNIQUE autoindexes reject without depending on CREATE SQL spelling.

Added `extra_unique_replace_indexes_cannot_delete_regions_during_the_first_checkpoint`
for regional and world extra UNIQUE constraints at generation zero, checking
both opens and the live checkpoint barrier with unchanged authority and empty
stored pages. Added a direct decoder-level regression,
`decoded_duplicate_page_rejection_preserves_the_first_authoritative_page`, so
duplicate rejection is exercised independently of the earlier schema barrier.
The decoder helper now checks map entry occupancy before insertion, preserving
the first decoded page even on failure. A Python SQLite metadata check confirmed
the expected built-in PK index shapes; Rust execution remains with integration.

## Integration fixture correction and focused execution

The first canonical candidate stopped because the external malformed-schema
editor inherited SQLite foreign-key enforcement and rejected its own hostile
FK fixture before Terra admission ran. Only that editor now disables enforcement
before its rewrite transaction; production validation is unchanged. The pinned,
offline focused library suite then passed **45 tests**, zero failed/ignored;
see `persistence-focused.log`. Final frozen-input suite evidence remains the
integrator's `v0.19-final-acceptance` receipt.
