# Independent source review — 0.22 source-package handoff

Reviewed the evidence-only
[source packer](../v0.22-delivery/package_source_022.py) and its imported
`release_contract`, `verify`, `package_desktop` and `verify_desktop` boundaries
before actual packaging. No archive was created and no frozen implementation
file was edited by this reviewer. This records source review, not a successful
final packaging run or a replacement for the generated archive-verification
report.

## Verdict

No blocking issue was found within the helper's stated consistency contract:
keep the source tree, baseline archive and accepted executable idle throughout
the operation, and use new external output/report paths. The integrator can
schedule invocation after final acceptance and document closeout are complete.

## Checks inspected

| Boundary | Source-review finding |
|---|---|
| Final acceptance | Requires the named 0.22 completed terminal record, an exact progress alias, explicit unchanged-input/report/log flags and a successful status. |
| Canonical gates | The imported validator checks the ordered canonical roster, not merely its length. The helper additionally requires 44 entries and explicit false timeout flags; exit codes and test counters are strictly typed. |
| Evidence association | Hashes every bound report and gate log; rejects duplicate evidence paths; checks the metadata/toolchain startup probe roster and successful bound probe logs. Frozen input and distinct-test records are required and re-read. |
| Current implementation and native binary | Compares the complete frozen input map/count to current implementation inputs, validates unique test identities and totals, and streams the currently named absolute native executable against its recorded hash. This state is read again at the end. |
| Baseline identity | Streams the exact supplied archive against the pinned 0.21 SHA-256, requires all 2,376 regular members and validates their namespace, bytes, sizes and Unix modes. It repeats the full baseline read after packaging. |
| Preservation policy | Every original member has a unique current destination. The only relocation is the old root delivery manifest to its named historical path. Missing originals and unapproved original changes fail. Current 0.21 product-review and validation copies must match the baseline exactly. |
| Permissions | Original ZIP Unix modes are assigned through the explicit path mapping, including the relocated prior manifest. Historical current-document copies retain their original modes; new files retain their observed source-tree modes. |
| Current source inventory | Rejects nonregular files and symlink directories, applies the explicit generated-file exclusions, validates portable names and records every retained file's bytes/hash/mode. Each file is read again while writing, and the entire source inventory is compared again afterward. |
| Exclusive publication | Baseline, source, ZIP and report paths are separated. Existing output/report paths are rejected, and exclusive file creation prevents replacement if another entry appears before writing. Imported modules do not invoke their command-line workflows. |
| Archive verification | Requires the exact member set without duplicates, validates portable paths and fully reads each member for CRC, hash, size and file type/mode. Generated manifest bytes and mode are checked separately. The entire output archive is verified a second time after final source/baseline/acceptance/native revalidation. |
| Completion report | A success report is written only after those checks. The documented possibility of a retained ZIP after failure means the report and process outcome govern delivery, not the ZIP's mere existence. |

## Explicit limits retained

These checks establish the observed archive's consistency and association with
the accepted source/binary evidence. They do not authenticate the publisher,
make a concurrently changing filesystem into an atomic snapshot, certify
power-loss durability, or qualify native behavior on an unexecuted platform.
Final archive names, counts, hashes, readback and successful invocation belong
to the integrator's separately generated delivery evidence.

## Acceptance-directory provenance

The integrator-directed handoff now binds the packer to
`docs/verification/v0.22-release-acceptance`. Only that directory literal changed;
the reviewed receipt, canonical-44, source/native, preservation and final-readback
checks are unchanged. The first canonical attempt remains retained as a failed
run. The second directory, `v0.22-confirmed-acceptance`, remains untouched after
an interruption with 42 completed zero-exit gates and no terminal receipt.
The [interruption record](acceptance-interruption.json) records no captured
application failure, no surviving verifier/workload process, an unrecorded cause
and all 378 frozen inputs still matching. That incomplete run cannot authorize
packaging. The fresh directory must complete all 44 gates and supply its own
successful terminal record before invocation; this provenance note does not
claim acceptance or package completion.
