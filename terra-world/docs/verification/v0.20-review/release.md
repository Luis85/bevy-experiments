# 0.20 release, QA and developer-handoff review and plan

Baseline: 0.19 source, reviewed 29 October 2026. No Rust or native execution is claimed by this specialist.

## Selected finding

The runner publishes progress and terminal status to the same mutable `gates.json`, then prints success without reading back its final bytes. In 0.19 a successful process and complete logs coexisted with a retained running/44 snapshot. The cause remains unestablished; this review does not assert that the filesystem or synchronization caused it. A final overwrite alone leaves no separately named terminal publication contract and no immediate readback check.

## Implementation plan

Publish `completed-gates.json` exactly once, atomically and without replacing an existing file, after writing and verifying the compatibility `gates.json` alias. Flush staged bytes before linking and verify exact published bytes before the runner reports success. Publication errors must produce a failed process result. Preserve any observed failure record for diagnosis. Use the completed filename in downstream CI/package instructions. Add fault-injection regressions covering complete success, existing destination/symlink preservation, failed linking, and changed published bytes. Run delivery Python tests and include them in the existing Python delivery gate.

## Other product findings and boundaries

- Current compatibility and full 44-gate/count admission are sound foundations; actual executed platform evidence remains mandatory.
- Windows execution, clean-machine dependencies, signing/SBOM/update/rollback and production license inventory remain open (G02/G11).
- A complete local receipt is not publisher authentication or evidence against coordinated fabrication. Immediate readback cannot guarantee future retention or protect against a later external writer. This change narrows an observability/publication gap (G01/G11/G17), not every storage or trust gap.
- No new public service or telemetry is introduced. Native windowed QA remains blocked by the executor's Unix-socket policy and is not retried here.

## Implemented and checked

- Added `publish_terminal_report` and `publish_completion` in verify.py: final legacy-alias exact readback, a separately named exclusive completed receipt, staging-file flush/fsync, atomic same-filesystem link and exact completion readback. Errors return failure before a success announcement; evidence remains available for diagnosis.
- CI now consumes `verification-results/completed-gates.json`; desktop packager's usage text names that terminal path. Legacy completed passed receipts remain readable by the packager.
- The existing delivery test gate discovers `test_*.py`, including seven new publication fault/success regressions and the eighteen incumbent packaging checks. Canonical 44/Rust-test gate names are unchanged.
- `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts -p 'test_*.py' -v`: **25 passed**, zero skips on Linux. No Rust build was run by this owner.
- Directory-fsync/power-loss guarantees, future external modifications, filesystem hardlink support on other platforms and native Windows execution are not established. If terminal publication is unsupported/fails, verification fails visibly rather than claiming a completed publication.

Root should update current developer commands to package from `completed-gates.json`. The 0.19 reconstructed acceptance and its original snapshot remain historical; this change does not rewrite their provenance.

## Cross-review correction

Storage review found that a passed legacy alias could still package after terminal publication failed. Corrected desktop admission: when `completion_record` is present it must be the fixed safe `completed-gates.json`, a contained regular nonsymlink file must exist, and its bytes must exactly match the supplied report. Both names are retained in desktop evidence and hashed by its package manifest. Reports without the field retain explicit historical compatibility. Three publication-admission regressions cover missing terminal, unsafe/null/stale terminal and exact successful inclusion. Final delivery Python result: **28 passed**, zero skips. This strengthens the terminal contract; it does not erase the process-exit requirement or authenticate external receipts.
