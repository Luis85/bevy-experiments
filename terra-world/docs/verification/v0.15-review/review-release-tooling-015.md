# Release tooling and claim cross-review — 0.15

Reviewer: navigation_increment_015. Project: `/workspace/scratch/0fdc9aa7bc4c/work/terra-world-next`.

## Result

**No blocking P1/P2 acceptance, packaging or claim defect found in the reviewed changes.** This is a read-only tooling/contract review, not a declaration that final release gates or native acceptance have completed. The gap report explicitly labels release acceptance in progress; final source, native, package and preservation receipts remain parent-owned.

I read `scripts/verify.py`, `scripts/desktop-payload.json`, the relevant existing desktop packager checks, `docs/REVIEW_GAPS_v0.15.md`, the corrected desktop commands and `/tmp/terra-build/release_preservation_015.py`. I compared the new parsers with report-producing source and retained focused logs, checked payload existence/reference closure, parsed Python syntax, and inspected the exact prior ZIP/index shape. No Cargo, project mutation, added gate or new behavioral test was used.

## New release gates

### Navigation cluster

- The gate invokes the exact `autonomous_vehicle_passes_a_cluster_without_moving_parked_bodies_or_losing_its_load` integration case with `--exact --nocapture`.
- The expected identity `(terra-sandbox, navigation_clusters, case)` agrees with the harness and test source.
- Anchored extraction finds exactly one `NAVIGATION_CLUSTER_REPORT` in the retained focused log. The actual field names agree with the parser.
- The checker requires the three-obstacle case, 0.1-second physical steps, a bounded positive deadline, consistent step/time arithmetic, physical travel and near-goal arrival, lateral bypass, step/heading/path/source-query bounds, positive resumed continuation and the exact set of true conservation/support/contact/state assertions.
- The report is published into the shared evidence-artifact hash set. Broader cluster behavior remains covered by the ordinary workspace test gate; the report gate does not falsely claim to exercise all cases by itself.

Retained focused data inspected: 145 steps / 14.5 physical seconds; 75.48865786078075 m travelled; 0.26763031676563404 m remaining; 0.80000000156063 m maximum step; 3.367809295654297° maximum heading change; 3 maximum path points; 5,760 maximum source queries per update; 143 exact resumed steps. All satisfy the declared report checks. The focused source contains the assertions behind its report booleans.

### Managed launcher recovery

- The exact fully qualified managed original-loss test is selected with `--exact --nocapture`, with expected `(terra-demo, terra_demo, case)` harness identity.
- Anchored extraction finds exactly one `LAUNCHER_RECOVERY_REPORT` in `/tmp/terra-build/launcher-review-015.log`; actual fields agree with the parser and test emitter.
- The gate requires schema 1, generation 2, two principals, nonempty regional/resource/body counts and all required exact-archive, source/politics, profile/presentation, publication, original-backup and repeat-backup assertions.
- Actual inspected report: generation 2, 2 principals, 52 living bodies, 16 resources, 19 nonempty regions, 57 paged and 25 global records. It passes the gate's workload checks. The test actually removes the original world/input tree and compares the complete recovered archive and preserved source/presentation before printing its assertions.
- Bounded-result pinning and failure-path cases belong to the full demo test gate. The report's scope does not pretend its one journey replaces those cases or the native rendered journey.

### Comet asset chain

- Generator `--check` compares all four output files without writing; its `matched`, file count, GLB digest and byte-count fields agree with the new parser.
- Inspection checks the exact source GLB digest, positive model bytes and the `parked`/`rolling` clip set.
- Standalone and composed direct validation require the expected one-model or six-model set and a nonempty accepted identity.
- Recipe build requires six sources, the exact current recipe digest and equality with the published authoring report; runtime revalidation must accept the same built pack identity.
- Direct-demo identity and recipe-built identity remain separate, as they should: declarations and model paths differ even when source GLBs are identical.
- Existing source freezing, exact harness accounting, failed/ignored-test rejection, log/report hashing, final source comparison and accepted native-binary digest capture continue to cover these gates. Both reviewed Python scripts parse successfully.

## Desktop payload and commands

The curated payload contains **139 unique existing file paths**, with no missing entry. The standalone Comet manifest, combined demo manifest and composed recipe resolve every referenced GLB to an included path. I independently compared model digests from both direct manifests against the current files; no mismatch was found. The new current gap report and authoring/import/recovery guides are included. Original licenses and attribution are retained. Saved worlds, session capabilities and temporary authoring output are not introduced into the allowlist.

The desktop package remains the accepted native executable plus runtime content and guides. Operator/authoring executables and source generators belong to the companion full source project, as the delivery guide states; their absence from the runtime package is not a missing runtime dependency. Existing packager logic checks the accepted source manifest, captured evidence hashes, exact executable bytes, platform and curated contained paths, then verifies ZIP entries before new-path publication.

The corrected Comet example is valid for the code's argument contexts:

```sh
./terra-demo --play --region-store /absolute/worlds/comet-review.db --asset-pack crates/terra-bevy/assets/transport-demo.pack.json
```

It tells the reader to use a new store. Existing managed profiles may instead be passed directly with `--world-profile` plus `--asset-pack`, which is permitted by the actual argument exclusions. The guide explicitly says `--menu` does not accept the pack argument. There is no new menu asset-picker claim. The selected manifest and its dependencies are included in the payload.

## Preservation script

- The script pins the delivered 0.14 ZIP's SHA-256 and verifies ZIP integrity before using it.
- The actual prior archive has 959 entries, all under `terra-world/`, with no directory entries, matching the script's prefix-removal/count assumptions.
- It requires every prior path to remain and validates the original-fixture/pack hash index plus protected historical reports, captures, versioned Markdown and JSON against retained bytes.
- I confirmed that the protected original-input index is the exact one in the prior ZIP and contains 90 entries. Although the script reads that index from the current tree, the enclosing historical comparison protects that index file against change in the same check.
- It separately requires archived PRODUCT_REVIEW_v0.14 and VALIDATION_v0.14 bytes to equal the old current documents.
- New-file counts are explicitly labeled point-in-time; later added release receipts are not falsely claimed to be part of an earlier inventory. The exclusions are explicit, and the script produces a new output path rather than overwriting an existing receipt.

I did not execute or modify the preservation script during this review. The final preservation receipt still needs its normal parent-run check after release assembly; source-archive/desktop extraction acceptance remains a separate step.

## Gap-register claims

The current 0.15 report retains all G01–G18 rows and keeps broad production content, uncoached progression, platform/devices, WAN, deployment recovery and global architecture open. New improvements are scoped to bounded stopped-body planning, the optional Comet model/rig, and supported local managed recovery.

The report now discloses the baseline's unbounded flat support versus the final bounded flat fixture. It separates original traffic fixtures, local timing, artificial cold-resource extent and native/hardware acceptance. Comet remains optional; default wildlife, the procedural cockpit, empty seats, fixed livery and absent human/boarding/audio work are explicit. The paint-polish claim matches the actual selected-appearance/stale-context guard in `driving.rs`. Local recovery does not claim offsite encryption/retention, public-server metadata recovery or service RPO/RTO.

The pending-acceptance preface is appropriate and must remain until the final receipts exist. No additional blocker or optional gate expansion is proposed by this review.

## Subsequent static review: final source-archive helper

The launcher/build owner requested a separate static review of `/tmp/terra-build/package_source_015.py`. I read the helper and compared its expected evidence/data shapes with the existing release records and the actual local-save, regional-archive, sandbox-state and session-identity structs. **I did not execute the helper, import its code, assemble a ZIP or edit project files.**

One concrete P2 was found and corrected before use: final output names were hard-linked before their ownership was registered, with fallible staging unlink/directory fsync in between. An ordinary error at that point could leave an untracked source manifest, ZIP or external receipt and prevent a clean retry. The owner now captures device/inode identity before publication and registers the final path immediately after a successful no-replace link. The ZIP path follows the same sequence. Cleanup compares that device/inode pair, so legitimate staging-link count/ctime changes do not suppress cleanup and an externally replaced file is left intact. I re-read and accepted this correction.

No additional concrete blocking defect was found in the reviewed controlled-release workflow:

- Required automated/native/desktop/release records must be passed for 0.15.0; the gate sequence matches the accepted verifier declaration, logs/reports are rehashed, exact source inputs are compared and native/desktop executable identities must agree.
- The exact prior ZIP supplies the 959-path baseline, 90 original input hashes and 560 protected historical hashes. Current preservation receipts and archived prior-current documents must match those contracts.
- Runtime directories and known private database/save/profile/wrapper/session payloads are excluded or rejected; matching accepted baseline fixtures, PEMs and logs remain deliverable only by exact prior digest. The supported runtime JSON signatures match the actual Rust envelopes.
- A new manifest covers every included file except itself. Source snapshots include file identity/change checks before, during and after assembly.
- The ZIP uses exclusive publication, the `terra-world/` prefix, exact member/path/order/count checks, CRC and every member's SHA-256. The external delivery record avoids a circular self-hash and is checked before successful completion.

These static conclusions do not substitute for the parent's final frozen-tree invocation and actual archive receipt. Both reviewer messages explicitly left execution with the parent after final evidence is complete.
