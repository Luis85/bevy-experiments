# Delivery record — Terra World 0.22.0

## Accepted release and native identity

The [completed release receipt](../v0.22-release-acceptance/completed-gates.json)
records **44 successful gates**, **1,043 distinct Rust tests**, **1,144 Rust test
executions** and **46 Python tests**. All 378 implementation inputs, 36 published
evidence artifacts and 44 gate logs remained unchanged. The accepted run finished
at **2026-10-29T22:15:38.769213+00:00** with runner exit zero.

Receipt SHA-256:
`7992f2fb0821faaa1f72d96f0d5b69f0fea18cd3acbc593214109dc1db893085`.

The native walkthrough and accepted native build use identical executable bytes.
The [executed association record](../v0.22-native/confirmation-input-association.json)
and [independent review](../v0.22-review/test-only-source-review.md) account for
the sole two-byte test-expectation correction between their source maps.
Native executable SHA-256:
`34dc6fe5c906d556fcf182968df0a92742079008c5eb26cc94d652a69a041cd6`.

The earlier failed fixture attempt and interrupted attempt remain in their
original directories. Their partial results are excluded from accepted totals.
The [validation report](../../VALIDATION.md) describes those attempts and the
limits of each kind of evidence.

## Linux desktop — verified

| Property | Actual result |
|---|---|
| Archive | `terra-world-0.22.0-linux-x86_64.zip` |
| Target and build | Linux x86_64; optimized dev profile, not a release-profile benchmark |
| Archive size | **39,608,922 bytes** |
| ZIP members | **261 regular files** |
| Extracted size, including manifest | **172,543,314 bytes** |
| Manifest inventory | 260 payload files plus the manifest itself |
| Unix modes | 259 files at 0644; native executable and launcher at 0755 |
| Passive shipped verifier | Passed, process exit 0, no stderr |
| Explicit shipped smoke verifier | Passed, process exit 0, no stderr |
| Actual native `--help` | Exit 0; 3,227 output bytes; 0.008099 seconds within a 20-second deadline |
| Complete extracted payload after each check | Unchanged files, directories, hashes, sizes and modes |

Desktop ZIP SHA-256:
`b10ba4887b63ba3b5c7ee71dae818fa29161e61a42cb1a8baa6ea1a86319c1cb`.

The independent specialist extracted into a fresh directory outside the source
project, verified every ZIP path/type/CRC/size/hash and Unix mode, and ran the
**shipped** verifier from that clean extracted root. The packaged terminal
receipt and its alias match the current accepted terminal record byte for byte;
the extracted native binary matches the manifest and accepted external binary.
The archive was rehashed after the checks and remained unchanged.

Actual records:

- [Package creation](desktop-package.json).
- [Passive inspection](desktop-passive-verification.json).
- [Explicit help smoke](desktop-smoke-verification.json).
- [Independent complete extraction/inventory review](independent-desktop-verification.json).

The independent report's SHA-256 is
`00da3ff4b3e42f7c89722f73163885a51d5eb6efe7ad90ce0d6ac9de5a057ffd`.
The report includes each extracted file's bytes, hash and mode. The manifest's
own hash is checked separately, so the verifier's count of 260 is consistent with
the archive's 261 members.

Before normal launch, run these from the complete extracted desktop folder with
Python 3.11 or later. If redirecting reports, put them outside the clean folder:

```sh
python3 scripts/verify_desktop.py
python3 scripts/verify_desktop.py --smoke-help --timeout-seconds 20
./start-terra.sh
```

The smoke check exercises command-line startup. The separately recorded
[native journey](../v0.22-native/README.md) supplies window/input/save/reopen
observations for the identical executable.

## Complete source — paired verification record

The source handoff is `terra-world-0.22.0.zip`, paired with the separately emitted
`terra-world-0.22-source-verification.json`. The paired report supplies its exact
archive size/hash, final member count, original-file accounting, and successful
byte/mode readback. It is produced only after the complete source tree is frozen
and the final archive has been verified. Keeping the archive's own identity in
a separate report avoids changing the bytes being identified.

The [source packager](package_source_022.py), independently
[reviewed here](../v0.22-review/source-package-cross-review.md), requires the actual
successful 44-gate terminal record and exact current implementation/evidence/
native identities. It re-reads the complete current source inventory and the
actual delivered 0.21 ZIP, preserves all **2,376 original members** through an
explicit mapping, and rejects missing or unexplained changes.

The sole original-path relocation is the generated prior delivery manifest to
`docs/verification/v0.22-review/baseline-delivery-manifest.json`. Exact prior
current review/validation copies remain versioned. Original assets, source data,
configuration, authoring inputs, licenses, screenshots and historical evidence
remain; intentional source/document edits are named in the preservation policy.
Original ZIP Unix modes are restored after extraction. The archive's new root
`DELIVERY_MANIFEST.json` records every included source file and excludes itself.

The packager verifies every output member's bytes and mode, re-reads the actual
baseline/current source/accepted executable, and verifies the archive again.
A failed invocation may leave its owned output; only its successful final report
qualifies that artifact. Consult the paired report for the actual outcome and
counts, rather than inferring success from a filename or tool review.

## Qualification boundaries

Release 0.22.0 uses wire 11 and TLS `terra-world/11`; clients and hosts upgrade
together. Domain-save, regional and asset schemas remain 1. Current source and
catalog identity checks still apply. The exact accepted Linux executable uses
Rust 1.99.1; the documented minimum remains 1.97.1.

The native observations used Linux X11/Vulkan software rendering at 1440×900 and
1100×700 with 150% text. This package inspection adds no physical GPU, Windows,
controller, assistive, WAN, sustained-player or power-loss qualification. Exact
hashes and readback establish consistency, not publisher authentication. Windows
clean-machine proof remains G02/P1 in the [complete gap register](../../REVIEW_GAPS_v0.22.md).
