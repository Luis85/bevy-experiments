# Persistence, recovery and operations review — 0.19 candidate

Baseline: supplied 0.18.0 source archive. Reviewed 29 October 2026.
Method: source and documentation review; this report does not claim fresh native,
hardware, power-loss, Windows or deployment qualification. Rust execution is
pending integrator toolchain availability.

## Assessment

The existing design has strong foundations: one bounded worker request, exact
revision installation, durable command barriers, WAL-aware bounded backup,
no-replace recovery destinations, immutable canonical partition proof, complete
cold-state validation, and explicit terminal uncertainty after a lost worker.
The docs correctly distinguish the last committed checkpoint from uncommitted
play and distinguish hashes from publisher trust. Preserve these contracts.

## P1: supported-schema admission omits storage constraints

`crates/terra-persistence/src/lib.rs::validate_schema` verifies object names and
column names through `PRAGMA table_info`, but does not verify column storage
types, nullability, primary-key ordinal, generated/hidden columns, foreign keys,
checks or table DDL. The comment and public recovery contract promise rejection
of an externally extended or incompatible schema.

A rewritten `regions` table with the same five named columns and no primary key
passes this gate. `read_partition` then inserts rows into `partition.pages` and
`encoded_pages` without checking whether the same regional key was already
present. Repeated rows can therefore collapse into one authoritative page while
the declared byte count includes all rows. The last page wins; an incompatible
schema is accepted as a canonical domain checkpoint. Ordinary checksums do not
establish row uniqueness. A schema rewrite with added foreign-key behavior is
another unsupported contract admitted by the current names-only check.

Recommended bounded closure:

1. Validate the supported SQLite table contract structurally with
   `PRAGMA table_xinfo`, including types, nullability, primary-key ordering,
   default values, hidden/generated fields and foreign-key absence. Check table
   flags/index contracts where they affect storage semantics. Avoid brittle
   exact-SQL string equality so legitimate existing schema-1 stores remain valid.
2. Reject duplicate region keys explicitly while decoding, even after schema
   validation, before replacing either map entry.
3. Keep validation at both opens and transaction boundaries; keep read-only
   maintenance, archive, inspection and live authority consistent.
4. Regression tests: remove/reorder the regional primary key; add an unexpected
   hidden column or foreign key; insert duplicate rows and adjust declared byte
   count; mutate schema after open; prove no checkpoint acknowledgement/state
   installation, preserve the prior generation and retained source bytes.

Acceptance: current legitimate schema-1 databases and normal backups still open,
export, restore and checkpoint; altered-schema reads and writes fail without
acknowledging a new generation. No new schema number or content migration is
needed for validating the existing contract.

## P2: recovery remains local and host-scoped

The bundle path intentionally rejects live/historical/custom-catalog/server
sources. This is well disclosed in `RECOVERY_BUNDLES.md`; do not label the whole
operational-recovery gap closed. A production handoff still needs a named owner,
retention and confidentiality policy, offsite copy location, backup frequency,
restore rehearsal, and measured RPO/RTO on its actual storage. These depend on a
deployment decision; preserve an explicit acceptance checklist rather than
inventing a successful operational policy.

## P2: maintenance latency remains unqualified

`RegionWorker::finish`, destructor join, full restore and final checkpoint can
wait for disk; the ordinary completion poll remains nonblocking. Backup bounds
the SQLite copy loop but later validation/sync can exceed that copy deadline.
This matches the current docs. A measured maintenance duration budget and
slow/full/failing storage campaign remain necessary before claiming bounded
shutdown or storage stall closure. Do not weaken durability or return success
before final commit to improve perceived responsiveness.

## P2: platform and failure evidence

The source includes valuable abrupt-process and unfinished-transaction tests.
They do not establish physical power-loss resilience, filesystem sync behavior,
Windows ACL confidentiality or Windows hard-link alias detection. The current
Unix-only inode comparison in `validate_output_path` explicitly leaves Windows
hard-link alias protection unimplemented. Physical Windows create/play/save/
reopen/recover and intended storage qualification remain separate P1 acceptance
for this user's machine.

## Implementation priority

Implement the supported-schema admission and duplicate-key closure first, with
regressions on writable and read-only paths. Preserve existing wire/save/asset
contracts. Then run the persistence suite and affected recovery suites. Keep
operational and platform qualification as concrete open acceptance work; no
source-only change can establish those outcomes.
