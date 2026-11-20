# Independent review: post-native protocol fixture correction

## Verdict

**PASS — exactly two test-only expected-value corrections.** The reviewed
change updates the expected Hello and Welcome JSON protocol values from 10 to
11 inside `#[cfg(test)] mod tests`. No production byte, test assertion, field,
fixture capability or serialization path changed. This is a source review; it
does not turn the retained failed acceptance run into a pass or establish a
rebuilt executable's identity without checking that artifact.

The reviewer compared the current
[`protocol.rs`](../../../crates/terra-net/src/protocol.rs) directly with the
[native-confirmed copy](../v0.22-native/native-confirmed-protocol.rs), using byte
comparison, an exact two-replacement reconstruction and a unified textual diff.
No implementation file was edited and no Cargo command was run by this reviewer.

## Exact comparison

| Check | Observed result |
|---|---|
| Complete file lengths | Both 24,237 bytes |
| Native-confirmed file SHA-256 | `a8ad77fb81c863a5e966363630f0bdf2077224781234d616ed107516a1aa301a` |
| Corrected file SHA-256 | `4b5001995d242310206464f6ad1e903694be1d982e578ad148f30d489cabfe73` |
| Test module boundary | One `#[cfg(test)] mod tests`, beginning at line 347 |
| Production prefix | All 12,286 bytes before that boundary are identical |
| Production-prefix SHA-256 | `530e38a702c0b262ab524b3b739d203c2507bd2660889ace1968a76820c2d8bd` |
| Changed byte at zero-based offset 15,820, line 446 | Expected Hello protocol: `10` → `11` |
| Changed byte at zero-based offset 16,014, line 450 | Expected Welcome protocol: `10` → `11` |
| All other file bytes | Identical |

The unchanged production prefix already declares `PROTOCOL_VERSION: u16 = 11`.
Its Hello/Welcome types, custom Debug redaction, welcome validation and other
production protocol logic are unchanged. The changed expectations are in
`capability_debug_redaction_preserves_private_wire_handshakes`; applying only
those two literal replacements to the retained copy reproduces the complete
current file exactly.

## Fixture strength retained

The fixture still constructs Hello and Welcome using `PROTOCOL_VERSION` and
the same synthetic capability. Its unchanged assertions cover:

- Standard and pretty Debug formatting for both message variants and nested
  aggregates: neither the whole token nor its eight-character prefix may
  appear; `[REDACTED]` and the field name must remain visible.
- Useful nonsensitive diagnostic context, including the Hello variant and the
  Welcome actor ID.
- A guest Hello with no capability: both Debug formats show `None` and do not
  falsely show a redaction marker.
- Exact complete JSON values for both private wire handshakes, including the
  capability needed on the wire and all other expected fields.
- Deserialization of each expected JSON object into its typed message and
  serialization back to the exact same JSON value.

No assertion was removed, weakened, ignored or replaced with a partial match.
The correction aligns the explicit wire expectations with the already deployed
protocol-11 implementation; it does not substitute the constant into those
expectations and thereby make that part of the fixture self-referential.

## Source and native evidence association

The reviewer independently rebuilt the same 378-input inventory used by the
release verifier and compared it with the first final run's retained
[`source-hashes.json`](../v0.22-final-acceptance/source-hashes.json). There are no
added or removed inputs, and only `crates/terra-net/src/protocol.rs` differs.
That map's old protocol hash matches the native-confirmed copy exactly.

| Source inventory | Canonical SHA-256 |
|---|---|
| Native-confirmed / first final run | `ec3fb5123a82cd1d4e6f5351d8f04503d305359628a44f61d8e032d9eca5902d` |
| Corrected test fixture | `261b8840abe316cb873cb9561e52115b453fe4826a430a609b20166351cf4510` |

The [native confirmation process](../v0.22-native/confirmation-managed-reopen-process.json)
retains the first source identity and executable SHA-256
`34dc6fe5c906d556fcf182968df0a92742079008c5eb26cc94d652a69a041cd6`.
The integrator must compare the executable produced by the corrected canonical
build with that hash. If identical, the retained native captures can be
associated explicitly through the identical executable and this one test-only
input change. This source comparison alone does not claim executable equality.

## Acceptance record handling

The first canonical attempt remains a
[failed terminal receipt](../v0.22-final-acceptance/completed-gates.json).
Its [headless log](../v0.22-final-acceptance/headless.log) identifies this exact
fixture's JSON equality failure: the constructed message serialized protocol
11 while the stale expected object contained 10. The log reports one failed
test in that crate; its earlier Debug-redaction assertions had already run
before that failing equality assertion. This does not imply that the complete
workspace run finished successfully.

The focused rerun and a fresh complete canonical run are integrator-owned
execution evidence. The corrected complete run belongs under
`docs/verification/v0.22-confirmed-acceptance`; the earlier failure remains
retained. No additional interface correction or native-polish claim follows
from this fixture adjustment.

## Evidence-association helper review

**Source review approved after one correction.** The reviewer inspected
[`associate_acceptance_022.py`](../v0.22-native/associate_acceptance_022.py),
SHA-256 `a1eea4072cc99510586968347bb96938f279b677c4c6a9939c5f02fd7dce13a4`.
Its checks bind the completed successful 44-gate receipt and exact canonical
roster/results to the current source map; retain the actual earlier failed
receipt and its hashed source map; match that old map to the native process's
unchanged source identity; admit only the two-byte protocol expectation change
after the unchanged production prefix; and require the actual accepted
executable hash to equal both the acceptance receipt and native process hash.
The output is exclusively created and expressly identifies an association,
rather than a new interface execution or replacement acceptance receipt.

Initial review found that optimized Python could discard the assertions that
enforce those checks. The integrator added an explicit `if not __debug__` error
at the beginning of `main`, before receipt reads or output creation. Source
reinspection confirms that correction. The reviewer also inspected the
integrator's [optimized-mode refusal record](association-helper-optimized-refusal.json):
the finite `python3 -O` invocation exited 1 with the expected guard message and
created no association output. This is helper validation, not an additional
release gate or Rust test.

The reviewer did not execute the association helper or Cargo. Successful
association still requires the fresh complete acceptance receipt and actual
executable equality when the integrator runs the helper after acceptance.

### Acceptance directory update after an interrupted attempt

The [interruption record](acceptance-interruption.json) reports that the second
attempt at `docs/verification/v0.22-confirmed-acceptance` stopped without a
terminal receipt: 42 completed gates recorded zero exit codes, no application
test failure was captured, no surviving verifier/workload process was found,
and its 378 source inputs still matched the frozen implementation. Its cause
is not recorded. That incomplete directory remains retained and cannot
authorize acceptance or packaging; the observations are not a successful
44-gate result.

The integrator started a fresh complete run at
`docs/verification/v0.22-release-acceptance`. At the integrator's request, this
reviewer changed only the helper's `accepted_dir` literal to that directory.
`failed_dir` remains `docs/verification/v0.22-final-acceptance`, preserving the
actual failed protocol-fixture receipt and old source-map association. Reversing
that single literal substitution reproduces the previously reviewed helper's
SHA-256 exactly; all validation, optimized-mode refusal and binary-equality
checks remain unchanged. The updated helper SHA-256 is
`ea005c6722eb1f4d01767e5db40c30b26c65275ac277fdedd6e8a4e63e575e16`.
Earlier review hashes and attempt history above remain intact. No old evidence
or implementation input was edited, and this reviewer did not execute the
helper or Cargo. A fresh successful terminal receipt is still required before
association.

### Executed association closeout

**Verified after completed acceptance.** The integrator executed the helper
successfully, producing
[`confirmation-input-association.json`](../v0.22-native/confirmation-input-association.json)
with SHA-256 `d227478d3bf51e9bbd94245eb8b2658692f816079aa5eee616e400d5fd416d02`.
This reviewer independently checked it against the actual
[successful terminal receipt](../v0.22-release-acceptance/completed-gates.json),
SHA-256 `7992f2fb0821faaa1f72d96f0d5b69f0fea18cd3acbc593214109dc1db893085`,
the old and accepted source maps, retained native process record, both protocol
files and current executable bytes.

All 44 canonical gates have zero exits, no timeouts and no failed or ignored
Rust tests. The 44 captured gate-log hashes and 36 evidence-artifact hashes
match the receipt. Its distinct-test inventory contains 1,043 unique Rust
tests; the gate totals contain 1,144 passing Rust executions, and the separate
Python packaging log reports 46 tests and `OK`. The current 378-input source
map equals the accepted map. Only the already reviewed two protocol-expectation
bytes differ from the native-confirmed map; the production prefix is identical.

Hashing the actual accepted executable confirms exact equality with both the
acceptance receipt and retained native process:
`34dc6fe5c906d556fcf182968df0a92742079008c5eb26cc94d652a69a041cd6`.
This closes the source/binary association for the retained native observations
without claiming another interface execution. The earlier failed receipt and
interrupted attempt remain unchanged historical evidence. This reviewer
performed read-only checks and appended this result; no helper rerun, Cargo
execution or implementation edit was performed.
