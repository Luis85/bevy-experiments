# Persistence reliability review — 0.17 candidate

Date: 2026-10-29. This is new candidate implementation evidence. The accepted
0.16 recovery, persistence, package and native receipts remain historical
evidence; this review does not relabel them as 0.17 acceptance.

## Confirmed defect

The regional worker's `try_complete()` and `finish()` reported a disconnected
result channel without clearing their pending request. The local Bevy adapter
cleared its own pending request and paused authority, but the underlying worker
continued to report pending forever. A subsequent checkpoint returned `Busy`;
restore and exit repeatedly encountered the lost channel. The SDK therefore
could not distinguish work still running from a worker that had stopped.

Simply clearing pending is insufficient. A checkpoint can commit before the
acknowledgement is lost. Treating a later `finish()` as an idle success or allowing
maintenance against the retained pre-completion state would conceal that
uncertainty.

## Applied behavior

`crates/terra-persistence/src/worker.rs` latches an explicit terminal failure for
disconnected request/results, poisoned result access and mismatched completion
identity. It clears the false pending state and closes further submissions.
Polling, final barriers, submissions, exports, backups and direct maintenance
continue to report the same error until the caller replaces the worker after a
validated reopen. Last acknowledged status stays unchanged. Ordinary completed
SQLite errors retain their existing retry behavior.

The read-only `RegionWorker::requires_reopen()` query exposes this distinction.
`RegionalPersistence::requires_reopen()` forwards it for the native adapter. The
adapter's status line and blocked gameplay-command message direct terminal
failures to close/reopen. Recoverable storage failures retain their
checkpoint/restore guidance. Domain schema, regional schema, source identity and
host authority remain unchanged.

## Focused regressions

The new private `crates/terra-persistence/src/worker_tests.rs` tests discard the
completion receiver of real submitted work. They do not add a production
failpoint or bypass SQLite. The worker finishes or loses its channel naturally;
the caller must handle the missing acknowledgement.

| Regression | Required result |
| --- | --- |
| Lost checkpoint acknowledgement | The actual second checkpoint remains exact across the complete archived state, metadata, identity, catalog and generation. Host-visible state and last acknowledged status stay retained. Pending clears, every continuation path fails, and a validated reopen recovers generation 2 before the next successful generation 3 save. |
| Lost hydration acknowledgement | The retained partial host state and complete committed archive stay unchanged. A final barrier fails explicitly, repeated barriers cannot silently succeed, and a validated reopened worker can hydrate again. |
| Disconnected idle request channel | A new submission fails with reopen guidance, does not become pending, cannot admit maintenance or later successful exit, and leaves the committed archive unchanged. |

Both executed full persistence library runs passed **38 tests**, with zero failed
or ignored tests. The final run includes the worker accessor and all three new
regressions, plus the existing real SQLite transaction rollback/retry case,
contention, stale writers, corruption rejection, crash recovery, backup bounds
and dormant-state retention. The command was:

```sh
cargo test -p terra-persistence --lib --locked -- --nocapture
```

See `persistence-worker-tests-final.log` and `reliability-tests-receipt.json` for
the final crate-level execution and exact worker input hashes. The initial run
is retained in `persistence-worker-tests.log`. The Bevy status/copy changes are
outside this crate-only test run; root integration validation must compile and
exercise them before full candidate acceptance is claimed.

## Adapter and exit trace

Source review confirms that Bevy's `poll_regions()` clears adapter pending on a
worker error, sets the persistence block, pauses world authority and exposes the
error. The native flow's `checkpoint_finished()` returns an error when that
block is set, so the Saving panel returns to the open session without claiming a
successful save. `checkpoint_on_exit()` propagates a failed final barrier as an
error exit. The network commit barrier polls the worker before permitting
mutation; a latched failure continues to stop that barrier.

This trace is source review, not a new native fault-injection or public-server
qualification. The focused tests establish the worker contract. Root integration
validation and any native interaction evidence remain separately recorded.

## Deferred review observations

Managed recovery already checks bounded canonical records, complete bundle
integrity, unchanged selection and checkpoint identity, separate output folders,
and final profile publication. No additional reproducible overwrite defect was
established in that path. Its accepted recovery tests remain useful regression
coverage; no speculative recovery rewrite was made.

Source-cache eviction enumerates and sorts all cache entries. This was recorded
as a review observation but left unchanged: this pass did not establish a
blocking permitted playtest workload requiring a cache redesign. The existing
source/persistence admission ceilings and regional-scale limits remain in force.
