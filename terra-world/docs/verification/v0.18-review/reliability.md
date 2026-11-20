# Reliability review and implementation — 0.18

Scope: regional storage worker poison recovery. Reviewed against attached 0.17 source; no diagnostic/UI changes.

## Finding and correction

A panic while the shared `RegionStore` mutex was held poisoned it permanently. The background worker converted that poison into an ordinary completion with a default `RegionStoreStatus`. `accept()` then replaced the last acknowledged status, cleared pending work, and allowed `finish()` to return idle success on the next call. Idle `export()`/`backup_to()` reported a generic lock error without making `requires_reopen()` true, so adapters could omit the terminal recovery path.

`requires_reopen()` now observes the persistent mutex poison as well as a latched channel failure. Idle entry points reject poisoned storage with explicit reopen guidance. Pending completions check poison before installing any status and enter the existing terminal latch, preserving acknowledged status and rejecting later barriers/submissions. Ordinary SQLite transaction failures retain their existing retry contract. No save, network or content schema changes.

## Regressions

Two new tests cover:

- A real mutex panic with an already submitted request. Synchronization deliberately holds the mutex while hydrate/checkpoint is submitted, then panics. Hydration uses nonblocking polling; checkpoint uses the maintenance barrier. Both must become terminal, retain the prior generation/status, reject every continuation, preserve the exact archive, and allow a validated reopen followed by generation 2.
- Idle mutex poison detected before read-only export, backup or maintenance. Reopen status is visible immediately; no backup file or maintenance side effect is produced, and later finish cannot imply success.

Independent source review also checked that idle read-only paths expose the terminal state and that ordinary completion errors were not all made terminal.

## Validation

Executed from `/tmp/terra-v018-integration/terra-world` with the Rust toolchain and dependency cache configured by `/tmp/terra-build/environment.sh`:

```sh
cargo test -p terra-persistence worker::tests --offline
```

Result: **5 passed, 0 failed, 0 ignored; 35 filtered out**. This includes the three retained channel-loss tests and both new mutex-poison regressions. See [focused execution log](reliability-worker-tests.log). `rustfmt --edition 2024 --check` passed on both changed Rust files.

The discarded scratch-tree test runs saw baseline files restored by workspace synchronization and are not acceptance evidence. Broader release acceptance is recorded separately by the integrator.

## Limits

These regressions establish the mutex poison contract in finite SQLite fixtures. They do not qualify arbitrary disk stall duration, OS write cancellation, abrupt process/disk corruption, Windows execution, deployment retention or offsite RPO/RTO.
