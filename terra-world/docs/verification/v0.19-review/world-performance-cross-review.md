# 0.19 cross-review: storage admission and launcher regression harness

Reviewer: world/performance owner, reassigned independently of those implementations. Reviewed source on 29 October 2026; no Cargo compilation or peer source edits were performed by this reviewer.

## Storage: blocking follow-up found

**P1 CR-WP01 — additional automatic UNIQUE indexes can preserve the new column contract while changing writes.**

At review time `validate_schema` in `crates/terra-persistence/src/lib.rs` validates visible objects through `sqlite_master WHERE name NOT LIKE 'sqlite_%'`, then exact `table_xinfo` column/type/nullability/PK/hidden attributes and foreign keys. `CREATE TABLE regions (..., PRIMARY KEY(level,x,y), UNIQUE(level) ON CONFLICT REPLACE)` retains all those attributes. Its additional constraint is implemented by a hidden-from-that-filter `sqlite_autoindex_regions_2` with origin `u`.

The existing checkpoint upsert targets `(level,x,y)`. On an otherwise empty generation-zero store with this admitted schema, two different regional keys at one level can both report successful insertion, while the second insertion deletes the first due to the unexpected UNIQUE conflict policy. The complete outgoing payload inventory can therefore be acknowledged despite missing durable pages. Testing a rewritten nonempty checkpoint alone can miss this because its earlier payload-inventory validation usually rejects the already-collapsed rows.

A standalone Python/SQLite reproduction was executed during this cross-review:

```sql
CREATE TABLE regions (
 level INTEGER NOT NULL, x INTEGER NOT NULL, y INTEGER NOT NULL,
 payload BLOB NOT NULL, digest TEXT NOT NULL,
 PRIMARY KEY(level,x,y), UNIQUE(level) ON CONFLICT REPLACE
);
```

Observed `table_xinfo` matches the expected five-column contract exactly, the filtered schema-object list contains only the table, and `PRAGMA index_list(regions)` reports one `pk` and one extra `u` index. Executing the same production upsert for `(16,1,1)` and `(16,2,1)` leaves only `(16,2,1)` after two successful statements.

**Required correction:** validate the admitted automatic-index contract in addition to columns/foreign keys: the existing regions primary-key index is allowed, extra UNIQUE constraints/indexes are rejected. The `world` INTEGER primary key needs no auxiliary UNIQUE index. Cover the generation-zero externally rewritten database and retain active world, acknowledged/stored generation and row count on rejection. Root and the persistence owner were notified immediately. Exact CREATE TABLE spelling is unnecessary.

Other inspected boundaries are sound within that remaining correction: validation runs inside the final immediate write transaction as well as read paths; descriptor/generation/data-version checks remain; duplicate decoded page identities are explicitly rejected; the positive schema-one backup/read-only/writable reopen regression checks real compatibility. The duplicate-row negative fixture is rejected by schema admission first, so it establishes defense at the public storage boundary rather than direct execution coverage of the deeper duplicate-decoder guard.

## Launcher: duplicate bundle corrected during review

**P1 CR-WP02 — adding `TextLayout` to the common `text()` bundle initially left a second `TextLayout` in `button()`'s label bundle.** Bevy bundle admission rejects duplicate component types, so normal button creation and the new launcher test could panic. Root was notified. On reinspection, the button now uses `p.spawn(text(title, 14., TEXT))`; the redundant outer bundle was removed. This resolves the source-level finding; the central real test run remains the runtime confirmation.

The launcher regression harness supplies the resources consumed by `rebuild` and button activation: `MenuState`, optional `PreviewImage`, `InputSettings`, `PlayerInput`, `InputFocus`, `Actions` and `ButtonInput<KeyCode>`. It adds only the rebuild system and manually triggers the relevant Activate observers. No font/layout/window/GPU systems are run, so absence of renderer assets or a Window is consistent with this structural test's declared scope. Text input component requirements are supplied by their component bundles; no additional directly read runtime resource was identified in this harness.

The status assertions correctly inspect ancestry beneath the shared ScrollArea and assert the fixed footer remains outside it; action identity checks use the real Activate observer and bounded Actions queue. This does not establish actual text measurement, layout fit at 150%, Tab traversal or assistive/device access. Those acceptance limits remain correctly stated in the implementation note.

## Follow-up gate ownership

Root owns the actual Rust runs. Suggested affected gates remain `cargo test -p terra-persistence --locked` and `cargo test -p terra-demo --locked status_layout_tests`. Do not freeze acceptance until the extra-UNIQUE schema finding is addressed and both the storage and launcher regressions actually pass.

## Corrected implementation re-review

Reinspected the persistence owner's corrected source before the integrator's final frozen-input gates. **CR-WP01 is addressed at source level.** `validate_schema` now reads `pragma_index_list` without the `sqlite_%` name exclusion: the world table must have no auxiliary index, and regions must have exactly one unique, nonpartial index with origin `pk`. `pragma_index_xinfo` additionally requires the actual primary-key columns in level/x/y order. Consequently the reproduced extra `UNIQUE(level) ON CONFLICT REPLACE` autoindex cannot pass admission; this check is shared by reads and the final checkpoint transaction.

The new `extra_unique_replace_indexes_cannot_delete_regions_during_the_first_checkpoint` regression creates the adverse constraint separately on each table at generation zero, proves the fixture requires multiple regional keys, then checks read/write opens and the existing handle's checkpoint reject. It asserts unchanged active authority, acknowledged/stored generation zero and zero inserted rows. This covers the previously missing first-checkpoint boundary.

`insert_decoded_page` now uses `BTreeMap::entry` occupancy before insertion. The direct regression injects a conflicting page with an altered feature edit, verifies rejection leaves the first page intact, then admits an independently keyed page. This closes the previously noted execution-coverage gap without exposing an API or changing serialized state. The existing positive schema-one backup/read-only/writable reopen test remains.

No additional source blocker was identified in this correction. Runtime/compilation acceptance remains the integrator's actual gate receipt; this re-review did not run Cargo or change peer source.
