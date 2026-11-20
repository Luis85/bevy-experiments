# Delivery acceptance and source-helper peer review

Reviewer: reliability and delivery specialist, 30 October 2026. The release
coordinator owns the new source helper and final packaging. This reviewer made
no implementation changes during this review. The WP07 standalone smoke code
was implemented by this reviewer and independently reviewed by the coordinator;
it is not presented here as a new independent review of the reviewer's own work.

**Disposition: no blocking defect found in the source helper or the reviewed
acceptance-to-package handoff.** Final archive construction, independent
extraction and the real canonical receipt were still pending at this point.

## Sources inspected

- [0.23 source handoff helper](../v0.23-delivery/package_source_023.py)
- [Canonical release contract](../../../scripts/release_contract.py)
- [Canonical runner and terminal publication](../../../scripts/verify.py)
- [Desktop packager](../../../scripts/package_desktop.py)
- [Standalone extracted-package verifier](../../../scripts/verify_desktop.py)
- [Current preservation policy and observed baseline](preservation.json)
- The corresponding Python regression suites and retained
  [50-test polish result](reliability-python-polish-tests.json).

Reviewed source-helper SHA-256:
`df3184eefeee94abee03c710819f61d8dd0aea65c7eccc321904133791f5b43b`.
Later changes to that helper require renewed review or an explicit recorded
delta; this identity is a review observation, not authentication.

## Required acceptance and freshness

The source helper uses the designated
`docs/verification/v0.23-release-acceptance` directory and requires a successful
**0.23.0** `completed-gates.json`. The progress alias must contain the exact same
bytes. A partial, failed, differently versioned or missing terminal receipt
cannot authorize archive creation.

The helper requires exactly **44 gates** and calls the shared contract validator,
which checks the exact ordered canonical roster. Gate outcomes require integer
zero exits, zero failures and ignored tests, correctly typed nonnegative counts,
and executed tests for each Rust test gate. The source helper additionally
requires an explicit false timeout field on every gate. Its recomputed passing
execution total must equal the receipt.

Every claimed report and gate log is read as a regular file, hashed and compared
with its accepted digest. Duplicate JSON keys and duplicate evidence bindings
are rejected. The required metadata/toolchain startup probes must have successful
outcomes and bound log hashes. Source-manifest and distinct-test identity
evidence are mandatory: exact counts, unique crate/harness/name identities and
per-crate totals are checked.

Freshness is established by comparing the accepted frozen implementation map
with `source_inputs()` from the current checkout, and by hashing the accepted
native executable at its recorded absolute path. This is stronger than trusting
the receipt's release label alone. The helper repeats the complete accepted
state check after constructing and rereading the archive. The new helper does
not turn the preceding focused 70-test or 238-test candidate records into
canonical acceptance.

These checks associate a locally observed receipt with unchanged inputs and
artifacts. They cannot authenticate a publisher or make a forged, consistently
rewritten set of local evidence intrinsically trustworthy. The release
coordinator must retain the real runner invocation and observed completion.

## Complete preservation and archive publication

The baseline is pinned by its complete ZIP SHA-256 and **2,790** regular Unix
file entries. Both its compressed bytes and every uncompressed member are read;
namespace validation rejects duplicate, escaping, case-aliasing and nonportable
paths. The old root delivery manifest is preserved at its explicit historical
path. The two replaced current documents must also have exact historical copies.

Every original member must exist through the documented path mapping. A changed
original requires a named reason in the reviewed allowlist, and a relocated
historical manifest cannot be silently edited. Original Unix modes are restored
in the ZIP, including those of historical current-document copies. New source
files retain their observed modes. Missing or unexpected original changes abort
the operation.

The current inventory includes all source-tree files except the named build,
version-control and Python-cache exclusions. It rejects symlinks and special
files, validates portable names, and excludes a pre-existing root manifest so
the generated one cannot conceal its predecessor. Individual reads compare file
identity, type, size and timestamps before and after opening and reading.

The source ZIP and verification report require distinct, new paths outside the
source tree. The ZIP is created exclusively, populated with the full inventory
and generated manifest, flushed, then independently reopened. Every entry's
bytes, size, file type, mode and namespace is compared with the intended
inventory. The helper next rereads the entire current source inventory, the
actual baseline and accepted state, then verifies the entire output ZIP again.
Only after those checks does it exclusively publish a passed report.

A failed run may leave its new output ZIP for diagnosis; the helper documents
that fact and does not publish a success report. The output is not an atomic
snapshot of a concurrently edited filesystem, a power-loss certificate or an
authenticated software distribution. Keeping all packaging inputs idle remains
part of the procedure.

## Desktop and standalone verifier relationship

The desktop packager independently checks the canonical gate contract, compiler
and compatibility contract, exact frozen inputs, evidence bytes, executed test
identities and accepted native binary. It restricts runtime data to the reviewed
payload allowlist, generates platform-specific entry points, and verifies the
complete temporary ZIP before exclusive publication to a new filename.

The standalone verifier checks a bounded exact extracted-file inventory,
portable path namespace, regular files without hard-link or reparse aliases,
file hashes, native architecture and the receipt/binary association. Each
claimed evidence digest must be bound to the extracted inventory. It remains a
passive consistency check unless `--smoke-help` is explicitly requested.

The standalone verifier is intentionally not the canonical gate runner. Its
internal-consistency result does not re-execute the 44 gates or authenticate the
receipt's author. Actual release acceptance is established upstream, then
associated with the packaged bytes. The final independent extraction should
verify the real package's member names, modes and hashes, compare its native
hash with the real canonical receipt, and run the packaged verifier and bounded
help check on the matching host.

The canonical runner's trusted build/test command capture remains distinct from
WP07's bounded desktop help workload. This review does not claim the WP07 byte
bound applies to all Cargo logs or changes the existing runner's platform-wide
process cleanup semantics.

## Executed negative admission check

Before the canonical terminal receipt existed, this reviewer invoked the actual
current source helper against the real checkout and pinned baseline with new
external temporary output paths. It rejected the missing
`completed-gates.json`, returned exit code **1**, emitted no stdout, and created
**neither a ZIP nor a success report**. No acceptance fixture was substituted
into the source tree.

The exact command, timestamp, helper hash, observed error tail and absence checks
are retained in [source-helper-preacceptance-check.json](source-helper-preacceptance-check.json).
This proves the negative admission path only. Successful packaging and final
archive identities require the later real acceptance and delivery records.
