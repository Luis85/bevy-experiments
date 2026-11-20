# Independent cross-review — hydrate admission and restore consent

Reviewed the current 0.20 source; no peer files were edited and no compilation
was performed by this reviewer.

## Storage activation barrier

The full-residency hydrate shortcut now calls the incumbent complete
read_partition(true), which validates schema, descriptor, generation and the
empty-store invariant inside its read transaction. Valid cache reuse still
checks SQLite data_version/connection changes before returning the immutable
proof; same-generation external corruption therefore invalidates reuse. The
returned committed state is deliberately not installed over live edits. Failure
cannot update authority generation/residency/status or successful checkpoint
metrics on this path. Three regression tests cover regional/global/metadata and
schema corruption, initial generation-zero schema mutation, complete serialized
live-state conservation and positive repeated-cache-hit/live-edit/checkpoint
continuation. No new source-level blocker identified. Cache correctness does not
establish storage latency tails or hostile concurrent-host isolation.

## Coordinated restore review

Both world Load and paused Restore route through the same explicit review.
Requests do not enqueue Load; Cancel is initial focus, review captures private
snapshot target/context, Confirm consumes the one review and queues the retained
ReviewedRestore, then returns to the paused session. Review action filtering
prevents unrelated session actions from silently changing the reviewed target;
repeated or stale confirmations cannot replay a consumed review.

The SDK intent validates source-world identity, player, restore revision, save
and regional paths, backend mode and generation before normalization to Load.
JSON restoration reads bounded bytes, compares exact retained bytes and validates
the save before installing it. Regional restoration checks reviewed generation
inside restore_validated's callback, before changing the store cursor, and checks
host metadata/state before apply_restored_save. Existing raw Load remains an
explicit trusted host API; this is native player consent, not malicious-host
isolation. Regional review identity assumes authoritative generations; it is not
a forensic signature of arbitrary externally reauthored but checksum-consistent
same-generation databases.

Source review found no additional blocker in these boundaries. The restored
source remains paused through the flow's owned virtual-time policy; actual
rendered focus/Tab behavior, hardware and Windows remain separate qualification.
UX implementation/tests were still being completed during this inspection, so
only central final receipts can establish final executed coverage. The F12
restore_queued observation should include LoadReviewed (owner/integrator already
coordinating that matcher); it must not report a reviewed queued restore absent.
