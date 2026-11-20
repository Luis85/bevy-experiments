# Persistence, recovery and operations review — 0.20

Baseline: source 0.19. Review 29 October 2026. Scope is source-grounded storage
integrity and existing local recovery/operations contracts; no Windows or offsite
qualification is inferred.

## Selected defect and implementation plan

`RegionStore::hydrate` short-circuits when `restored && fully_materialized`.
Unlike its composition path, it checks only descriptor and generation before
admitting the caller's complete state. Same-generation external corruption of a
committed page, global ledger or metadata, and unsupported schema changes, can
therefore pass this source-registration/host-edit activation barrier. A freshly
initialized generation-zero store takes the same shortcut.

Route this shortcut through the existing complete `read_partition(true)`
admission. That path validates schema, descriptor, generation and empty-store
invariants; its guarded cache avoids re-decoding unchanged checkpoints and
external writes invalidate that cache through SQLite data_version. Keep live
state, residency/status, generation and successful checkpoint metrics unchanged
on rejection. Do not install the committed snapshot over valid live edits.

Regression matrix: restored complete state with live edits rejects same-generation
regional/global/metadata corruption and schema extension, preserving full caller
state/status and stored corruption without acknowledging a checkpoint; initialized
empty store rejects schema/header/regional-record edits; unchanged repeated hydration retains live
edits, uses the existing read cache and can checkpoint afterward.

## Other product perspectives and retained gaps

Named backup/import/recovery copies, exact descriptors, atomic SQLite transactions,
private exports, bounded backup retry and worker terminal-error semantics remain.
G15 transition/maintenance latency tails still need measurement; guarded cache
reuse is correctness evidence, not a latency promise. G16 retention, offsite
confidentiality, ownership and deployment RPO/RTO remain product/operator work.
Windows ACL/filesystem behavior and physical power-loss qualification need the
actual environment. No schema/save/wire contract change is planned.

## Execution boundary

Cargo execution is centrally owned by the integrator. This reviewer will format
source and supply test names; only the integrator's fresh receipts establish
actual compilation and test results.

## Implemented

The full-residency shortcut now uses `read_partition(true)` before validating the
caller's live state. Three new tests cover four committed corruption classes,
three empty-store invariant changes, and valid cached repeats plus live-edit
checkpoint continuation. Pinned rustfmt completed; no Cargo execution was run by
this owner. Integrator command: `cargo test -p terra-persistence --lib --locked
--offline`. Scope remains integrity admission, not storage-latency improvement.
