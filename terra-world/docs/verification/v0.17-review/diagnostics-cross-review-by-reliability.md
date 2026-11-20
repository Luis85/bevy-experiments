# Independent diagnostics review — reliability pass

Date: 2026-10-29. Scope: read-only review of `telemetry_capture.rs`, `output.rs`
and `performance.rs` during the 0.17 candidate implementation. This is separate
from accepted 0.16 evidence and from later executed tests/native acceptance.

## Finding and correction

The new public capture status could retain `pending_captures > 0` after a
diagnostic recorder disconnected. Polling a closed outcome channel produced no
acknowledgements, and later failed submissions did not retire the retained
count. The UI could therefore describe an already stopped writer as queued or
publishing indefinitely.

This was reported to the diagnostics owner. The owner added explicit pending
count/status retirement on queue/start/final errors and a deterministic stopped
writer regression. Source reinspection confirms that the error branch now
clears both fields while retaining the actual error. The diagnostics owner's
subsequent full native-demo test run passed **179 tests**, zero failed/ignored,
including that regression and the operating-system lock cases. This reviewer
rechecked the retained log's final result in
`diagnostics-native-tests-017.log`; the execution belongs to that separate record.

## Preservation and concurrency checks

The report writer creates a complete private lock marker in a temporary file,
locks that inode, and publishes the marker with an exclusive hard link. A
competing initial publisher cannot replace the winning inode. Subsequent
writers open that same persistent marker and require its operating-system lock;
file existence alone is not interpreted as a live lease. The lock survives
report-file replacement and the operating system releases it when the owning
process ends.

The final acknowledgement explicitly follows destruction of the retained
writer, so a menu-started next session can acquire the same report immediately
after final success. Contention and failed writes do not replace the earlier
report. Both report and derived lock paths pass protected world/session ancestor
checks before recursive directory creation. An unrelated existing lock file
must have the expected exact marker before use and is never rewritten.

History/sample/file limits are checked before publication. The summary uses the
validated last history sample, distinguishes running/error/final capture, and
does not infer current health from the existence of a report. Writer jobs and
acknowledgements remain bounded.

## Pause and exit semantics

Capture cadence uses real monotonic time. Paused virtual time, saved world-clock
pause, queued save/load actions, session phase and regional persistence status
are reported separately. Capturing these fields does not mutate authoritative
world state. Final diagnostics run after the regional/portable final checkpoint
set and distinguish an existing error exit from a normal exit request. Failure
to publish final evidence produces an error result and an explicit return notice;
an old report is not described as the current exit.

No additional blocking defect was established in this review. Windows lock and
filesystem behavior still requires actual platform execution; source inspection
and a Linux child-process test do not qualify Windows delivery. Administrators
replacing filesystem ancestors or lock markers while capture runs are outside
the supported quiescent-path workflow.
