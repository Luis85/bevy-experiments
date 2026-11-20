# Delivery, quality and developer-handoff review — 0.19

Reviewed 29 October 2026 against the supplied 0.18 source archive. This is a fresh source and executable-Python review; historical Linux native and Rust acceptance records are not current execution evidence.

## Findings and recommended bounded implementation

| Priority | Finding and evidence | Proposed closure and acceptance |
|---|---|---|
| P2 | `scripts/package_desktop.py:115–129` validates each reported gate but requires only one named `native-build` and a positive `distinct_passing_rust_tests` scalar. A report claiming `passed` with an incomplete gate list can therefore publish a nominally accepted package. The existing `test_package_desktop.py` publication fixture contains only this one gate and succeeds. The packager does not require or reconcile `distinct-tests.json` either. | Share the canonical gate roster with the runner; require exactly one of every expected gate, no extras/duplicates, and the hashed distinct-test identity manifest with count consistency. Test truncated receipts, substituted/duplicate names, missing identity manifest, wrong counts and successful complete fixture publication. This is local structural consistency, not authentication of a publisher. |
| P2 qualification | G02 Windows execution remains open, including for the user's Windows PC. CI has a Windows job, pinned compiler and x64 packaging checks, but neither configuration nor Python path validation establishes native platform acceptance. | Execute the pinned gates and clean-extraction create/play/save/reopen/recovery journey on the intended Windows host. Record Windows/GPU/input/runtime and retain its exact PE executable. This host cannot establish that acceptance. |
| P3 | `verify.source_inputs` freezes implementation/asset/data/verification inputs but deliberately excludes Markdown and root licenses. Desktop payload documentation/licenses are read at package time and hashed in the final manifest. Consequently accepted executable evidence does not freeze every user guide's bytes. | State this boundary accurately. If a later release needs fully frozen release documentation, snapshot a separate delivery manifest after final docs are written; do not mix historical evidence or mutate source while verification runs. |
| P3 supply chain | `THIRD_PARTY_NOTICES.md` and Cargo.lock disclose direct provenance and the resolved dependency graph; they do not constitute a complete per-dependency license inventory or authenticated publisher signature. The desktop notices also describe source distribution boundaries. | Keep source and desktop scope explicit. Future shipping release work should collect dependency licenses/SBOM, publisher trust/signing, an update/rollback policy and supported runtime matrix. Avoid claiming this review supplies them. |

## What is already sound

The packager uses a curated allowlist; checks source identity, compiler, binary hash/architecture/platform; rejects traversal, symlinks, case aliases, reserved Windows names and evidence shadows; verifies archived bytes; publishes through a new output path. The runner captures owned child output privately, publishes completed evidence, records test identities, preserves failed receipts and checks frozen source/report/log bytes. The compatibility checker validates current statements and nine workspace lock versions. Linux/Windows CI retains failed-job evidence. These mechanisms should be preserved.

## Fresh checks

- `python3 -m unittest discover -s scripts -p test_package_desktop.py -v`: 11 passed, 0 skipped on this Linux host.
- `python3 scripts/check_compatibility.py`: passed; release 0.18.0, wire 10, save 1, regional schema 1, Rust minimum 1.97.1 and acceptance compiler 1.99.1.

No Rust compilation, new desktop package, native session, Windows execution, signing, installer, clean-machine dependency resolution or GPU/input qualification was executed by this review.

## Product gap mapping

The recommended structural receipt closure strengthens G01/G11 (truthful, reviewable release evidence). G02 is still a blocking Windows acceptance task; G07/G08 require actual device evidence, and G13/G14/G16/G18 remain deployment/product decisions with their published acceptance criteria. Do not relabel all of those as closed by a packaging unit suite.
