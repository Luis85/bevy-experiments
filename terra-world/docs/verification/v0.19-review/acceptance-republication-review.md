# Acceptance receipt republication: independent delivery review

Reviewed 29 October 2026. This review distinguishes observed persisted files, the integrator's process observation and a one-off receipt reconstruction. No tests were rerun by this reviewer, and no implementation inputs were changed.

## Observed publication anomaly

The canonical runner's final persisted `v0.19-final-acceptance/gates.json` retained `status: running` with all 44 completed gate records. It lacked the final summary/native fields. The integrator reported command-session completion with exit 0 and `Verification passed: 946 distinct Rust tests.` This reviewer did not directly observe that process completion. The cause of the missing persisted final snapshot is unestablished; synchronization is a hypothesis, not a proven diagnosis.

Before republication, this reviewer independently preserved the running receipt outside the source tree. Its exact SHA-256 was `b420dcdfcf47089e3ed666a5bc03159e09220b446cc560cbf9f266f913fd138f`.

## Independently inspected file facts

- All 44 required gate records satisfy the strict roster and typed result contract.
- Their passing Rust execution counters sum to **1,032**, with zero failed or ignored tests; every counter reconciles with captured Rust summary lines.
- Every one of the 44 captured log hashes and the 33 then-recorded artifact hashes matched the receipt.
- The 369 frozen source hashes equal the current implementation inputs.
- The stored identity manifest contains **946 unique** crate/harness/name identities.
- Native build JSON identifies one actual executable, whose current SHA-256 is `b86ad4bc44685780b55c6f3eb8f32f88af4330f6f055d9f7d8d71e54339fe704`.

The inspected scratch publisher additionally reparses every test identity using Cargo target metadata and the real captured harness output, verifies exact equality to the stored sorted identity list, recomputes per-crate counts, validates every recorded report/log hash, checks all frozen inputs and binds the compiler-identified native executable. The publisher's source was reviewed; no gate or test execution is synthesized by it.

## Derived receipt and verdict

`v0.19-final-acceptance/accepted-gates.json` is explicitly an **independently verified reconstructed receipt**, not the untouched final output of the runner. It retains the original started time, records republication UTC separately, leaves the unavailable original finish time null, attributes process exit/terminal text to the integrator, and states that native visual qualification is false. The original `gates.json` is unchanged, with an exact hashed copy named `runner-progress-snapshot.json` included in the derived receipt's artifacts. This reviewer rechecked that both original copies match the independently preserved snapshot and that the derived receipt's hashes, counters, source manifest and native digest still reconcile.

The procedure is acceptable as a clearly labeled evidence republication. Hash consistency is local integrity evidence; it is not publisher authentication or proof that arbitrary forged test names were executed. The one-off reconstruction procedure should accompany the evidence for reproducibility rather than being described as a new ordinary verification run.

Rendered native QA was blocked before game launch by a Unix-socket permission error. CLI/help execution establishes executable loading and argument handling only. It does not establish current native layouts, player input, create/play/save/reopen, GPU behavior or Windows qualification. Historical native captures remain historical.
