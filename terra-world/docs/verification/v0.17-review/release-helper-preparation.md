# Release helper preparation — 0.17 candidate

Date: 2026-10-29. This records preparation and read-only validation of temporary
release helpers. **No 0.17 preservation receipt, source manifest, source ZIP or
external delivery record has been published by this preparation pass.** Root
must authorize the final freeze before either helper entry point runs.

## Exact baseline

The accepted archive is `output/terra-world-0.16.0.zip`, SHA-256
`985d95792ddcc6d84239c17ff1e79c12a79a4146aad3afa7c7567ec5afb436ca`.
Its digest and ZIP CRC were rechecked. Protection sets are derived from its
actual members and inherited preservation maps:

| Contract | Derived count |
| --- | ---: |
| Every prior source/evidence path retained | 1,488 |
| Original fixture/data/asset inputs kept byte-exact | 102 |
| Historical evidence and prior version records kept byte-exact | 1,068 |

Current `PRODUCT_REVIEW.md` and `VALIDATION.md` from that exact archive must also
appear byte-exact as `PRODUCT_REVIEW_v0.16.md` and `VALIDATION_v0.16.md`.
Original inputs are every non-Markdown file in baseline crate asset/data trees,
plus the inherited original-input map. History includes the full baseline
verification, screenshot and report trees, versioned Markdown records and docs
JSON, plus the inherited historical map. No earlier protected file loses its
protection merely because a naming rule changes.

## Prepared helpers

- `/tmp/terra-build/package_source_017.py` — SHA-256 `fed16cfd5447e63fe1b1a43b1bf29cd3745f3436500ffe21d5f1861bdf9f4cb3`.
- `/tmp/terra-build/release_preservation_017.py` — SHA-256 `6b39c095145f63d0f5d06fee2bd4703ee728081d6d949bbdeaab21c6332988fc`.

They use the exact 0.17 project, 0.16 baseline and new 0.17 output paths. All
inherited stable-read, exclusive output publication, exact ZIP member hash/mode,
source-freeze, referenced-evidence and private-payload checks remain. Counts are
derived from baseline maps and final evidence rather than copied from an earlier
release. Existing deliverables and source manifests cannot be overwritten.

## Gate and receipt validation

The assembler inspects the actual `scripts/verify.py` gate declaration without
executing it. The final record must contain that exact ordered gate list, and
the current runner must still include every inherited gate. Each gate's log is
rehashed; its passed/failed/ignored counts are independently recomputed from the
actual retained log. The hashed distinct-test manifest must contain unique
crate/harness/test identities matching its aggregate and per-crate counts.
Final release totals must reconcile to those real values. No fixed test or
source-input count is used as a substitute for the actual final run.

Native, desktop and release receipts retain the 0.16 structure with 0.17 version
and path fields. Their successful checks, exact gate/binary hashes, dynamic
source-input counts and every nested file reference are rechecked. Native's
`artifact_sha256` map uses paths relative to `docs/`. The desktop receipt must
bind the actual extracted-package archive path, bytes and digest. The release
receipt must bind the final gate/source/native/desktop/preservation records and
agree with the actual test and preservation counts. A nonempty all-true release
`checks` object is required.

## Preparation checks actually executed

Both helper sources passed Python syntax parsing. Read-only helper validation
confirmed the exact baseline CRC/digest and the derived protection sets. The
current runner retains all **43** inherited gate names. Independent log-count
validation of preserved 0.16 evidence recovered its actual **990** test
executions. A deliberately forged passing-test count was rejected, and an
isolated temporary runner with one inherited gate removed was rejected.

These checks did not execute current release gates, current-tree preservation,
packaging, native launch or Cargo. They establish helper preparation only. New
0.17 acceptance and exact source publication remain separate work after the
final freeze.
