# Independent source-packaging and preservation review — 0.24

Reviewed by the world/framework specialist on 30 October 2026. This review covers
`docs/verification/v0.24-delivery/package_source_024.py` and
`docs/verification/v0.24-review/audit_preservation.py`, including their use of the
current release-contract, receipt and namespace validators.

**Disposition:** three preservation findings were reported to the integrator,
corrected, and independently inspected afterward. No further blocking source
finding remained. The actual missing-acceptance check rejected correctly, and
eleven isolated preservation-policy fixtures passed. No final source package or
successful acceptance receipt was produced by this reviewer.

## Findings closed before packaging

| Finding | Why it mattered | Reviewed correction |
|---|---|---|
| Newly added paths were not classified again by the final packer. | A passed preservation checkpoint could become stale: a later unclassified addition would have been included by the complete inventory without receiving a declared allowance. | `preserved_files` now rejects any new path outside the exact named allowlist and the explicitly admitted current evidence prefix. The packer validates that prefix policy before creating output. |
| Prior release entries were not rechecked at package creation. | The changelog and release history are authorized current-document edits. A later removal of old entries could therefore pass the ordinary original-file edit allowance even though it violated the history-retention policy checked earlier. | `verify_prior_history` reads the actual baseline entries and current files again before output creation. It requires their historical entry block to remain byte-exact and the current file to match the fresh source inventory. |
| A mismatched readback of the generated baseline inventory did not affect checkpoint status. | The report recorded the equality result but could still use `passed_checkpoint` despite a failed inventory proof. | `audit_preservation` now includes `independently_read != original` in its failure decision. |

These corrections do not rely on weakening the historical preservation policy or
adding an allowlist entry after observing an unexpected change. The independent
fixtures exercise the first two corrected boundaries directly. The third
correction was inspected in the checkpoint's actual failure expression; the full
audit is left to the integrator after current documents settle.

## Acceptance admission reviewed

The complete source packer admits only the actual terminal 0.24.0 acceptance
directory. It requires a successful `completed-gates.json`, byte-identical
`gates.json`, the canonical 44-gate roster, current execution-completion metadata,
explicit no-timeout results, and consistent passing-execution totals. The shared
validator is invoked with `require_execution=True`; the receipt validator receives
actual bound log sizes.

Every named gate log and evidence artifact is read and checked against its
receipt digest. The required source/test evidence is present; startup metadata
and toolchain probes have the exact expected roster, successful completion and
bound logs. Duplicate JSON keys and duplicate evidence bindings reject.

The frozen implementation manifest must equal current implementation inputs and
its declared count. Distinct test identities must have the exact shape, unique
keys and consistent total/per-crate counts. The named accepted native executable
must exist as a regular file and match the receipt's digest. Imported verification
modules must belong to the chosen root. These checks associate the source handoff
with the accepted executable and evidence; they do not authenticate a publisher.

## Preservation and publication reviewed

The actual baseline archive is pinned by its complete digest and 3,092 regular
members. Its namespace, prefix, file types, member bytes and original modes are
read independently. The old root manifest must be absent from the current root
and retained exactly at its historical path. Prior current review/validation
copies must match their delivered originals.

The active-tree audit has an explicit dispatched-path allowlist and exact
historical copy checks. It reports original-mode differences as findings, without
granting a blanket extraction-normalization exception. A passing checkpoint is
still an active-tree checkpoint, not a final archive qualification.

The final packer inventories all admitted current source files, including
documentation, additions and historical evidence. Symlink/nonregular source
members reject. It applies the corrected new-path and release-history policies,
then writes exclusively to a new external output ZIP. The report must also use a
new external path. The previous generated root manifest is preserved at the
explicit relocation, while a new manifest records the actual current handoff.

Original ZIP modes are applied through the retained path mapping. Any source-tree
mode restoration is recorded explicitly; the generated archive is read back and
each member's bytes, type and mode are compared. The source inventory, actual
baseline, accepted receipt/evidence, implementation inputs and native executable
are re-read, then the archive is read and checked again. A success report is
written only after those checks finish. A failed packaging run may retain its
exclusive incomplete ZIP, requiring a fresh output path for another attempt.

## Independent execution evidence

### Actual absent-acceptance rejection

Executed the complete helper against the actual source tree and original 0.23
archive while `v0.24-release-acceptance/completed-gates.json` did not exist. The
exclusive candidate paths were `/tmp/terra024-world-source-negative-01.zip` and
`/tmp/terra024-world-source-negative-01-report.json`.

Observed **exit 1** with `Missing or escaped payload file: completed-gates.json`.
Neither candidate output nor report was created. The helper and its four imported
validator/tooling inputs remained unchanged throughout this invocation. This run
preceded the three preservation corrections; its receipt records those exact
earlier input digests rather than claiming it executed the corrected helper.

- [Exact command, timing, input digests and outcome](../v0.24-preflight/world-source-packer-negative-01.json)
- [Complete captured output](../v0.24-preflight/world-source-packer-negative-01.log)

### Corrected preservation-policy fixtures

Executed the committed independent probe script after inspecting the corrections:

```sh
python3 -B docs/verification/v0.24-preflight/world_source_policy_probes.py
```

Observed **11 tests passed**, exit 0. Reported unittest duration was 0.004 seconds;
the captured process elapsed time was 0.064375426 seconds. The helper, audit source
and probe script digests were unchanged throughout the invocation.

The fixtures admit exact preservation, declared new source, current evidence and
new release entries with intact history. They reject an unclassified addition, a
new historical evidence namespace, missing/modified protected originals, altered
retained manifest, changed historical copy, lost prior release entries despite a
fresh inventory, and changes after inventory. They also check explicit original
mode restoration. Tiny history fixtures live only in a cleaned temporary directory;
no synthetic acceptance receipt or complete release ZIP is used.

- [Reproducible independent probes](../v0.24-preflight/world_source_policy_probes.py)
- [Exact command, timing, input digests and outcome](../v0.24-preflight/world-source-policy-probes-01.json)
- [Complete captured output](../v0.24-preflight/world-source-policy-probes-01.log)

## Remaining final boundary

The integrator must still run the settled preservation audit and actual canonical
acceptance, package the accepted source/desktop, and retain their independent
readback results. This review does not assert any of those future results.

The checks establish consistency of observed bytes, portable member paths,
historical retention, modes and the accepted-binary association. They are not an
atomic filesystem snapshot under concurrent mutation, publisher authentication,
power-loss certification, a Windows execution result, or target-hardware
qualification. Keep the source tree and accepted executable idle for final
packaging, as the helper explicitly requires.
