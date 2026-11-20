# Independent 0.23 source-packaging review

Reviewer: world/framework specialist, 30 October 2026. This is a source and
preservation-policy review of the integrator's new
[`package_source_023.py`](../v0.23-delivery/package_source_023.py). It does not
claim that a final archive was built or accepted before the actual canonical
receipt exists.

## Finding

**No blocking defect found in the release adaptation.** The new helper retains
the previous source packer's checks and updates the release/baseline identities
and current evidence paths. The historical 0.22 helper remains byte-exact in the
independent baseline comparison.

| Boundary | Reviewed behavior |
|---|---|
| Release identity | `RELEASE` is `0.23.0`; the baseline is the exact delivered 0.22 ZIP with 2,790 regular members and SHA-256 `54ba5420db937056ba62e1f7861308a4ea540112f52ae500af10e92e897a739e`. |
| Prior manifest | Only `DELIVERY_MANIFEST.json` moves, to `docs/verification/v0.23-review/baseline-delivery-manifest.json`. The source tree must have removed the old root manifest before packaging. |
| Historical current reports | Exact prior `docs/PRODUCT_REVIEW.md` and `docs/VALIDATION.md` must survive as the corresponding `_v0.22.md` copies, with their original bytes and Unix modes. |
| Explicit edit policy | The helper reads the new `v0.23-review/preservation.json`, checks its baseline and exact relocation, and requires a named reason for each allowed original-path edit. Unlisted changed originals and every missing original reject packaging. |
| Actual acceptance | The helper requires `v0.23-release-acceptance/completed-gates.json`, its identical `gates.json` alias, the canonical 44-gate roster, explicit no-timeout results, validated execution counts, frozen input hashes and the actual accepted native executable. Old or candidate receipts cannot authorize the new archive. |
| Bound evidence | Receipt-associated source/test evidence, startup-probe logs and every gate log are re-read and checked against recorded hashes. Duplicate JSON keys and malformed or duplicate evidence bindings are refused. |
| Complete source | Inventory excludes only build/VCS/Python cache artifacts. Regular-file identity, bytes and modes are checked while reading; symlinks and nonregular members are refused. Every retained baseline file receives its original ZIP mode. |
| Publication | ZIP and final report use exclusive new external paths. The helper writes a new generated manifest, flushes and synchronizes the ZIP, then reopens and checks every member's bytes, CRC, size, regular-file type and Unix mode. |
| Final reread | After writing, the helper independently re-inventories all current source, rereads the actual baseline, validates acceptance and the native executable again, and verifies the entire ZIP a second time before writing a success report. |

The exact retained review, validation and manifest copies were independently
checked against the actual baseline ZIP before authorizing removal of the stale
root manifest. The [preservation policy and observation](preservation.md)
records the complete 2,790-member comparison, explicit edit reasons and mode
checks. New review helpers live under 0.23 paths; earlier evidence is retained.

## Verification and scope

The new helper parses as Python. A complete textual comparison against
`v0.22-delivery/package_source_022.py` showed only the intended release,
baseline-hash/member-count, historical-copy and current-evidence-path updates.
There is no changed packaging or acceptance algorithm in this adaptation.

The packer does not pretend that a live filesystem is atomic. The documented
operating precondition is an idle accepted source tree and executable; its
multiple identity/content checks detect observed changes during packaging.
An unsuccessful run can retain an exclusively created incomplete ZIP and must
use new output paths for a corrected run. Hash consistency does not establish
publisher authentication, power-loss certification, supported hardware or
Windows qualification.

Successful final execution remains the integrator's required release gate.
Its final independently re-read source archive report must be linked from the
current [validation](../../VALIDATION.md) and delivery handoff.
