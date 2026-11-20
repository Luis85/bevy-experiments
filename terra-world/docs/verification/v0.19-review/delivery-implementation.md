# Delivery implementation — 0.19

The desktop packager previously accepted a receipt claiming success with only its native-build entry and a positive scalar test count. This was reproducible in the previous valid-publication fixture. The closure is structural evidence admission; it is not publisher authentication.

`scripts/release_contract.py` now owns the 44-gate ordered acceptance roster. The verification runner validates its constructed commands against that roster before dispatch. The packager requires the identical complete roster before reading acceptance payloads, so missing, duplicated, reordered, substituted and extra gates cannot pass as complete acceptance.

The packager also requires the hashed `distinct-tests.json` artifact. Its nonempty identities must have exactly the crate/harness/name string fields, contain no duplicate complete identity, and reconcile exactly with the recorded total and per-crate counts. Counts must be positive integers, excluding booleans. Existing source/log/report hashes, namespace protection, binary/platform checks and atomic new-path publication remain.

The isolated packaging fixture now supplies a structurally complete roster and identity evidence. Meaningful negative cases reject a native-only report, missing/duplicate/unknown/reordered gates, missing identity sidecar, duplicate/empty/malformed identities and inconsistent scalar/per-crate counts. The positive case still publishes a ZIP and inspects exact payload and acceptance-record bytes. As before, those fixtures use synthetic executable header bytes and mocked local source/compiler bindings; they do not certify a native executable.

Fresh executed checks on 29 October 2026:

- `python3 -m unittest discover -s scripts -p test_package_desktop.py -v`: **18 passed, zero skipped**, captured in `delivery-tests.log`.
- Python compilation of release_contract.py, verify.py, package_desktop.py and test_package_desktop.py: passed.
- AST inspection of the real runner's 44 constructed gate names against the canonical roster: matched.

No Rust compilation, native desktop package, Windows session, GPU/device qualification or publisher-signing work was executed in this implementation. Existing G02/G07/G08/G11 platform/trust acceptance criteria remain open beyond this bounded evidence-consistency closure.

## Cross-review polishing

A reliability cross-review found that Python equality could accept `exit_code: false` as zero and missing failure/ignored counters as falsey. The shared result validator now requires integer-zero exit codes and mandatory nonnegative integer passed/failed/ignored counters, with zero failures/ignored. The 14 declared Rust test gates must report positive executed tests; other gates must not claim Rust executions. The runner's selected test gates are validated against the same shared declaration.

The runner publishes `passing_rust_test_executions` as the exact sum across gate records. Packaging requires this typed total to reconcile and bounds distinct identities by that execution total. Eighteen packaging tests include missing/boolean/string/float/negative counters, nonzero exits, failures/ignored tests, zero-execution test gates, phantom non-test executions, mismatched totals and distinct identities exceeding executions. The existing complete-publication fixture remains positive. Python compilation passes; source inspection matches the real runner's 44 gates and 14 Rust test gates. The historical 0.18 gate result shape satisfies the strict typed validator with 1,019 executions; this is a compatibility sanity check, not current-release acceptance.
