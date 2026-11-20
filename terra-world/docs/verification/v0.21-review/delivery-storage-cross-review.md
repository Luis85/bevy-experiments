# Independent delivery and storage cross-review — 0.21

Reviewer: world/simulation owner, reviewing the integrator's WP09 and the
reliability owner's WP07/WP08 on 29 October 2026. This review did not edit their
source. The in-progress R3 startup-witness work is explicitly outside this pass.
No Cargo command was run by this reviewer.

## Required correction: startup timeout cleanup and evidence

**P2, reproduced through the actual runner.** The initial WP09 implementation
routes acceptance gates through run_logged, which starts an owned process group,
stops it on timeout, retains partial output and returns timeout status 124.
However, the two startup probes in verify.main still use subprocess.check_output
with a timeout. They bypass that cleanup/publication path.

A temporary fake cargo executable was used to exercise the real metadata-startup
path without invoking Rust. It launched a real Python child, emitted partial
stdout, then stalled. With a 0.3-second runner deadline:

| Observation | Actual |
|---|---|
| Runner exit | 1 |
| Terminal receipt status | failed |
| Child started | true |
| Child wrote its delayed marker after the deadline | true |
| Partial startup log retained | false |

The child waited 0.8 seconds before its marker; the reviewer checked it one second
after the runner returned. The temporary fixture and evidence were automatically
removed after the check. The failed receipt correctly prevented success, but the
startup child escaped cleanup and its partial output was not retained. The
exception handler wrote startup.log only for CalledProcessError, not
TimeoutExpired.

**Smallest correction:** use the owned logged execution helper for cargo metadata
and rustc --version too; record separate startup-probe results and hashed logs
without changing the canonical gate roster. A timed-out startup must retain its
partial output and explicit classification, stop its child, and leave a failed
terminal receipt. The integrator accepted this correction and owns its changes.

Reproduction recipe: place a Python-script cargo executable first on a temporary
PATH, patch verify.ROOT to that temporary directory, and call verify.main with
--gate-timeout-seconds 0.3 and a new temporary evidence directory. The fake cargo
uses subprocess.Popen for a child that writes a readiness file, sleeps 0.8 seconds,
and writes a delayed marker; the parent prints partial metadata output and sleeps
five seconds. No real Cargo build is started.

## Additional platform qualification note

The initial Windows cleanup path discards taskkill's nonzero return code
(check=False) and can fall back to terminating only the command leader while
the timeout log says the process tree was stopped. This is source inspection,
not executed Windows evidence. The integrator should detect/report unconfirmed
descendant cleanup or retain that explicit limitation until Windows acceptance.
The Linux process-group cleanup was exercised by the existing focused tests.

## WP07 activation lifecycle

No new source-level blocker was found in the selected change.

- A pending worker request returns before the activation fast path, preserving
  one submitted revision and preventing early command admission.
- A matching hydrate completion opens the complete source/command batch;
  repeated intents reuse that admission.
- Source replay and commands mark residency work; the normal checkpoint system
  submits retirement and a matching checkpoint completion closes activation.
- Ordinary frames without new source/command work do not reopen hydration.
- Completion identity/revision checks precede state installation. Validation
  failures leave authority blocked rather than admitting queued mutations.
- Explicit restore already validates its state and opens a batch; it does not
  need an immediate second hydrate before that batch.

The new tests target full-state edits, generation zero, dormant records, explicit
restore, same-generation payload/schema corruption, sixteen queued commands and
actual source replay. Existing stale/lost-worker handling is retained. These
source conclusions require the integrator's fresh Rust tests; this reviewer
does not claim those executions.

## WP08 private snapshot publication

No new source-level blocker was found in the selected change. Unix mode 0600 is
set on the exclusive temporary before snapshot bytes are written, so replacing
an existing inode does not broaden its private mode. The original same-directory
rename and file synchronization remain; a failed rename preserves the previous
destination and attempts removal of the temporary.

The tests isolate umask changes in a subprocess, exercise first publication,
replacement and readback, and cover an unreplaceable destination. Unix modes do
not qualify Windows ACLs. The pre-existing ignored parent-directory sync result
is the separately recorded R4 policy issue; it is not newly solved by WP08.

## Executed focused Python checks

Command, from scripts:

    python3 -m unittest -v test_verify_execution test_package_desktop

**26 tests passed, zero failures or skips**, before the startup-probe correction.
This includes actual stalled-command/child cleanup, partial-log retention,
invalid timeout inputs, typed timeout admission, canonical roster checks,
terminal-receipt binding and package publication rejection. The separate real
runner startup reproduction above exposed a path those initial tests omitted.
Run the corrected startup regression before freezing acceptance.

## Follow-up after correction

The integrator routed both metadata and toolchain probes through run_logged,
retained separate hashed startup logs/probe outcomes, and changed taskkill to
checked execution with explicit unconfirmed-cleanup wording on failure. The
fresh /tmp/terra-python-021.log records 34 Python tests passing, including the
real startup-child timeout regression. The reviewer inspected that log and ran
focused checks for the mocked Windows-failure branch and qualified timeout log;
both passed. The reproduced startup defect and misleading cleanup wording are
addressed. See startup-witness-cross-review.md for the later WP08b cross-review,
its corrected metadata-output finding and remaining platform scope.
