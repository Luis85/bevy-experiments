# Independent delivery cross-review by persistence owner

Reviewed: `scripts/release_contract.py`, `package_desktop.py`, `verify.py`, and
`test_package_desktop.py` on 29 October 2026. No peer source edits or Cargo runs.

## What works

The shared 44-name roster matches the actual runner's ordered gates. Checking
that roster before execution and again before packaging prevents missing,
duplicated, renamed and reordered gates without introducing platform-specific
order drift. All paths run the same current gate list. The new
`distinct-tests.json` requirement correctly binds a hashed manifest and validates
its shape, nonempty string fields, duplicate identities, strict integer total,
and reconciled strict integer crate counts. Boolean global/crate counts are
correctly rejected. Source inputs include the new verification module, so a
change to the shared roster invalidates older acceptance input identity.

These checks establish structural consistency, not a signed publisher or
independent proof that an arbitrary evidence author ran the commands. The module
and current documentation state this boundary appropriately.

## P2 correction: per-gate counters remain optional and boolean exits pass

`package_desktop.py` checks `gate.get("exit_code") != 0` and truthiness of
`failed_tests`/`ignored_tests`. Thus `exit_code: false` passes as integer zero,
and absent counts pass as no failures. `passed_test_executions` is not required
or reconciled with the positive distinct manifest at all. A complete gate-name
roster with no per-gate test counts still reaches publication. The current
positive fixture actually omits all three gate test-count fields.

Executed Python-only reproduction: create the existing valid fixture; remove
`passed_test_executions`, `failed_tests`, `ignored_tests` from every gate and set
each `exit_code=False`; invoke the fixture's `run_package`. Actual result:
`missing_gate_counts_and_boolean_exits_accepted: passed`.

Recommended correction before source freeze:

- Require strict integer `exit_code == 0` (reject booleans).
- Require strict integer nonnegative passed/failed/ignored gate counts, with
  failed and ignored exactly zero.
- Share the set of gates that actually run Rust tests and require a positive
  passing execution count for each. Non-test gates legitimately report zero.
- Require the distinct Rust count to be no greater than the sum of passing Rust
  test executions; duplicate executions across gates mean equality is not valid.
- Extend the positive fixture to include realistic per-gate counts and reject
  missing, boolean, negative and zero-for-required-test-gate cases.

The runner already emits all those fields and validates each selected Rust gate
against discovered test identities, so this should preserve genuine receipts.
Exact per-gate command/log identity re-derivation is a broader assurance level;
do not describe the structural patch as authenticating arbitrary fabricated logs.

## Correction re-review

Re-read the delivery owner's corrected source. The shared
`validate_gate_results` now requires strict integer zero exits, mandatory
nonnegative integer counters, no failures/ignored tests, and positive passing
executions on all 14 canonical Rust-test gates; non-test gates require zero.
The runner verifies its selected-test-gate roster, publishes the typed sum, and
the packager reconciles that sum and requires distinct identities <= executions.
This resolves the reproduced missing-counter/boolean-exit structural bypass.
No further blocker found in this bounded re-review. The delivery owner's
18-test Python run is recorded in its implementation note/log; this reviewer did
not rerun that suite. Arbitrary evidence authorship remains outside hash trust.
