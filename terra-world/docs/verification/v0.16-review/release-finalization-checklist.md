# Finalization order and evidence index — 0.16

Date: 2026-10-29 UTC. **Final acceptance pending at creation of this checklist.**
The first full-gate attempt exposed a new test's nonexistent method call; its
correction requires a fresh successful run. This checklist does not convert
partial gates, candidate captures or source review into release acceptance.
The final [release receipt](../../release-verification-v0.16.json) will identify
the completed results.

## Required order

1. **Freeze the implementation and complete the full gate run.** Preserve the
   failed attempt separately. The accepted run belongs at
   `docs/verification/v0.16-final/gates.json`, with its exact
   `source-hashes.json`, captured logs and report artifacts. Require all 43
   declared gates, zero failed/ignored tests, unchanged implementation inputs,
   and the exact native executable produced by the final build. Derive test
   totals from that run rather than candidate or previous-release counts.

2. **Confirm the final native executable.** Record corrected guidance/driving
   behavior, naming/recovery and the declared presentation/window scope against
   that binary and gate-record hash. Keep candidate and final observations
   distinguishable. Publish `docs/native-v16-verification.json` only with the
   actual recorded outcomes and retained evidence hashes.

3. **Finish and freeze the desktop-included documentation.** The explicit
   [desktop payload](../../../scripts/desktop-payload.json) contains **148 files**,
   including guides, licenses, configuration and runtime assets. All currently
   exist. Finalize `PRODUCT_REVIEW.md`, `VALIDATION.md`, `REVIEW_GAPS_v0.16.md`
   and the remaining included guides before packaging. The current review and
   validation pages must describe 0.16 rather than retain 0.15 acceptance text.
   The exact prior versions are already archived separately. Markdown is outside
   the implementation hash map, so the passing source freeze alone cannot detect
   a later guide edit. Keep final ZIP hashes and extracted-package results in
   their external receipts to avoid a circular embedded claim.

4. **Package, extract and confirm the desktop.** Use the accepted gate record
   and a new ZIP filename. Compare **every one of the 148 allowlisted files** in
   the archive/extraction against the finished source-tree bytes; this is not
   merely a check that each path exists. Check the complete package manifest,
   every declared payload hash, accepted executable, evidence files and launcher
   permissions. Run the declared extracted-package confirmation, then publish
   `docs/desktop-package-v0.16.json` with the actual archive hash/size and exact
   native/gate bindings. Do not edit allowlisted guides afterward. An archive
   requiring changed payload must be rebuilt and rechecked under a new output.

5. **Publish preservation, then the final release receipt.** Once the review
   evidence and documents are complete and stationary, run the session-local
   `release_preservation_016.py` once for
   `docs/verification/v0.16-final/baseline-comparison.json`. It requires the exact
   accepted 0.15 ZIP, 1,213 retained paths, 96 unchanged original inputs and
   804 unchanged historical evidence files. Then write
   `docs/release-verification-v0.16.json`, binding the final gates, source map,
   native record, desktop record and that preservation receipt by hash. The
   preservation inventory is explicitly point-in-time; final assembly rechecks
   every protected byte and includes the later release receipt.

6. **Assemble source last.** Keep both session-local helpers together:
   `release_preservation_016.py` imports `package_source_016.py`. Run the source
   helper only after all acceptance records pass and the entire tree is stable.
   It creates `docs/source-archive-manifest-v0.16.json`,
   `terra-world-0.16.0.zip` and the external
   `terra-world-0.16.0-source-record.json`. Existing outputs are never replaced.
   It independently rechecks evidence, source inputs, protected baseline bytes,
   private-state exclusions, archive paths/counts, CRCs, every member hash and
   Unix permissions. Do not add or edit project files during assembly.

7. **Independently verify the delivered archives.** Check exact hashes/counts,
   extracted payloads/modes, source-manifest coverage, all 148 desktop/source
   payload comparisons, executable identity and receipt bindings. Keep this
   post-assembly verification outside the archived tree; adding it inside would
   change the payload whose manifest was just finalized. No archive count or
   success should be inferred from the baseline's 1,213 files.

## Receipt shape and link checks

The source helper expects `native.release == "0.16.0"`, while the top-level
desktop and final release records use `version == "0.16.0"`. The nested desktop
package result uses `release`. Each accepted record needs `status: "passed"`.
Native/desktop `checks` must be nonempty maps of true booleans; keep counts,
limitations and descriptions in separate fields.

Native `artifact_sha256` keys are relative to `docs/`. Nested objects containing
both `path` and `sha256` use canonical project-relative paths. Dedicated native
executable and package-output string fields use the actual absolute artifact
paths. Preserve the binaries and archives at those paths through source
assembly so their hashes can be rechecked.

The desktop carries gate evidence under `evidence/`; detailed source links,
native captures and release receipts belong to the companion complete source
project. Keep that distinction clear when reviewing links from packaged guides.
None of these records establishes signing/trust, Windows execution, physical
GPU/input qualification or deployment capacity beyond the recorded scope.

## Existing review evidence

- [Release engineering review](release-engineering-review.md) and
  [13-check audit](release-engineering-audit.json): exact baseline, lockfile,
  compatibility, payload and no-replace/Unix-mode checks.
- [Native product cross-review](native-product-cross-review.md): seven candidate
  naming screenshots, independent 43-file preservation checks and read-only
  review of the guidance/driving correction batch.
- [Navigation implementation review](navigation-implementation.md): physical
  geometry matrix, bounded reconnect work, exact resume and remaining holds.
- [Guidance cross-review](guidance-cross-review.md) and
  [driving polish record](native-driving-polish.md): handoff/selection findings
  and the scope of regression evidence.

The two temporary assembly helpers were unchanged at this sequencing review;
their reviewed hashes remain in the audit JSON. No helper or source code was
edited or executed by this checklist pass.
