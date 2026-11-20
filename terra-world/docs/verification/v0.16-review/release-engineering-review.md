# Independent release engineering review — 0.16

Date: 2026-10-29 UTC. Reviewer: navigation increment subagent, independent of the
root release integrator. This review covers the declared verification workflow,
desktop payload, compatibility changes and temporary source-delivery helpers.
It is an implementation and focused helper review, before final release
acceptance and archive extraction. It does not certify unfinished native,
desktop or final-gate work.

The machine-readable audit is
[release-engineering-audit.json](release-engineering-audit.json). It records the
reviewed script/helper hashes and the point-in-time scope below. The final gate
record and delivery receipts must bind the actual frozen implementation again.

## Findings closed during review

| Finding | Risk | Resolution and evidence |
|---|---|---|
| The human generator gate still expected four outputs after the cabin model became a fifth output. | A correct generator would fail the gate, and the new runtime cabin file could be absent from the desktop bundle. | Root updated `scripts/verify.py` to require the exact five names, each file's SHA-256 and byte count, and added `character-pack/comet-cabin.glb` to `scripts/desktop-payload.json`. The reviewed allowlist has 148 distinct entries. |
| Human authored-pack identity was initialized twice. | Harmless duplication made the new gate sequence harder to review. | Root removed the duplicate initialization. |
| The 0.16 source helper's usage text named the 0.15 preservation helper. | Following the header could run the wrong release helper. | Corrected the temporary source helper to name `release_preservation_016.py`. All fixed release paths and the accepted baseline hash now agree. |
| The preservation helper checked for an absent output, then used ordinary text writing. | An intervening creation or dangling output symlink could cause an unintended replacement. | Reused the source assembler's regular-file/symlink checks and atomic hard-link no-replace publisher. Existing files and dangling symlinks are rejected; focused temporary-directory checks confirmed their bytes/targets are preserved. |
| Preservation used its own weaker inventory and loaded the previous comparison from the mutable tree. | The two release helpers could drift in exclusions or protected-input interpretation. | The preservation helper now imports the reviewed source helper's exact baseline reader and bounded snapshot. The prior comparison comes from the SHA-256-verified 0.15 archive. A second snapshot rejects mutation during the comparison. The output is restricted to the fixed 0.16 final comparison path. |
| ZIP verification checked payload bytes but did not verify stored Unix modes. | A lost executable bit could survive a byte-perfect archive verification. | Added regular Unix member and permission checks. A synthetic archive with correct bytes, manifest and executable mode passes; an otherwise identical archive with the executable bit removed fails. The source delivery record includes the mode check. |

The helper changes are confined to `/tmp/terra-build/package_source_016.py` and
`/tmp/terra-build/release_preservation_016.py`. The preservation helper now needs
its source-helper sibling in the same directory. These are local release
assembly tools, not new runtime dependencies. Neither final publishing entry
point was run as part of this review.

## Verification integration

The current runner declares **43 distinct ordered gates**. In addition to the
existing workspace, native, source distribution, economy, traffic, recovery and
live-session checks, the new increment retains the navigation geometry matrix,
continued guidance journey, named-world recovery journey and human asset
pipeline evidence.

The runner captures each command's output before interpreting its report,
checks exit/failed/ignored results, identifies executed Rust test identities,
and stores log and report hashes. A final source inventory includes Cargo files
and the implementation/configuration/automation inputs. A changed source map,
changed published report or changed captured log prevents a passing final
record. The native-build gate identifies and hashes the actual compiler-produced
executable. Distinct Rust identities avoid counting repeated retained journeys
as additional unique tests.

The navigation gate requires the five distinct authored geometry reports,
complete verification sets, 0.1-second physical steps, the shared twelve
additional corridor admissions, bounded physical movement/steering, the
129-waypoint cap, finite source queries and resumed execution. The test source
adds the two-kilometre reconnect callback bound, path-capacity case and resumed
patrol case; those execute as part of the same test target. Its per-case report
retains authored corridor widths, unavailable/restored timing and hold/reorder
semantics. This remains an explicit finite geometry matrix, not a general
route-planner guarantee.

Guidance and named-world recovery gates require their exact test identities and
one retained report each. Guidance checks the three-obligation journey,
physical walking, wallet/reward relationship, exact saved obligations and
nonmutating duplicate/stale actions. The naming gate requires two separate
recovered worlds and a one-file metadata-only rename with preserved progress,
source and backup bytes.

The human pipeline validates the standalone human pack, the combined seven-model
pack, a recipe-built pack and the exact identity accepted by the runtime
importer. The generator gate binds all five example outputs, including the new
cabin GLB. The existing transport example remains a separate original protected
input. Desktop packaging uses the explicit payload allowlist, so arbitrary
working directories, saves and caches do not enter through recursive bundling.

## Exact baseline and compatibility review

An independent read of the delivered source archive verified its CRC and
SHA-256:

`92a8c73750a58da22ad48f899d7c941eadb78a4af0894a51cacde26eb25aeb3f`

The audit independently reconstructed the protected sets and matched the root
contract. At the review point:

- All **1,213** prior source paths remain present as regular files.
- All **96** original input files retain their exact accepted bytes.
- All **804** historical evidence files retain their exact accepted bytes.
- `PRODUCT_REVIEW_v0.15.md` and `VALIDATION_v0.15.md` exactly preserve the prior
  current documents from the accepted archive.
- Only the nine workspace package versions changed in the lockfile, from 0.15.0
  to 0.16.0. External dependency resolution is unchanged.
- The read-only compatibility checker passes: release 0.16.0, wire protocol 10,
  TLS ALPN `terra-world/10`, domain save 1 and regional schema 1. The documented
  minimum Rust is 1.97.1 and the acceptance toolchain is pinned to 1.99.1.

The new human presentation fields require a 0.16 importer and are documented as
optional. Old manifests without them keep their existing identity/behavior.
Guidance remains derived view state; naming uses the existing profile title;
no domain-save, database or wire version increment is claimed by this release.
The final compatibility gate still needs to run on the frozen source.

## Packaging and evidence bindings

The source assembler fixes the 0.16 output names and exact 0.15 baseline. It
rejects noncanonical paths, symlinks, nonregular files, case-colliding paths,
oversized payloads and changing file identities. It requires passed automated,
native, desktop and release records before constructing a source archive. The
declared gate sequence must match the script; every captured log/report hash,
source input count, toolchain, accepted native binary and desktop archive is
rechecked. References in the final release record must agree with those same
records and their source freeze.

All baseline paths are retained; protected originals and history are checked
again during assembly. The source manifest covers every archived payload except
itself, and the external delivery receipt hashes the complete archive without a
circular embedded hash. Assembly verifies source snapshots before and after
packaging, member order/count, CRCs, every member's SHA-256 and Unix modes.
Publication uses no-replace hard links. Ordinary failure cleanup removes only
inodes owned by that invocation, leaving previous deliverables intact.

Private runtime exclusion has a declared scope. Named cache/save/user-data
directories are excluded; new databases, save/session files, private key files,
recognized serialized world/profile/recovery structures and serialized resume
capabilities are rejected. Exact accepted synthetic fixtures, TLS test material
and historical logs are deliberately preserved. These rules and the explicit
desktop allowlist address the project's known runtime state; they do not claim
to be a general secret detector for arbitrary third-party text.

## Focused verification and remaining acceptance

Thirteen focused audit checks passed, covering the independent baseline,
compatibility/lockfile comparison, 43-gate declaration, current allowlist and
publication/ZIP-mode regressions. Python helper syntax compiled in memory. No
Cargo build, broad test run, socket operation, native capture or final artifact
publication was performed by this review.

At the recorded review time the only absent desktop allowlist entry was
`docs/REVIEW_GAPS_v0.16.md`, which the release integrator still needs to finish.
This is recorded explicitly in the audit JSON and is not treated as a completed
desktop payload. The remaining release sequence is to finish current docs,
freeze and run all gates, perform native acceptance, package the desktop, bind
its extracted confirmation, publish preservation/release receipts, and assemble
the source archive. Independent extraction and byte/mode/receipt verification
of both final archives remains pending until those artifacts exist.
