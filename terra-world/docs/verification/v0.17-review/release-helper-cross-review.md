# Independent release-helper cross-review — 0.17

Review date: 2026-10-29. Read-only review by the diagnostics workstream of
`/tmp/terra-build/package_source_017.py` and
`/tmp/terra-build/release_preservation_017.py`, including their complete delta
from the 0.16 helpers. Neither helper was assembled, published or executed via
its `main`/preservation `check` entry point. No helper source was edited.

## Outcome

No blocking defect was established in the reviewed helpers. Dynamic counts
derive from the exact accepted baseline and validated current evidence; they do
not silently remove an inherited release gate or reduce preservation scope.

An independent read-only Python check, separate from the helper implementations,
verified the accepted 0.16 ZIP SHA-256:
`985d95792ddcc6d84239c17ff1e79c12a79a4146aad3afa7c7567ec5afb436ca`.
It derived **1,488 prior paths**, **102 original input files** and **1,068
historical evidence files** from that archive and its inherited preservation
receipt. At this review checkpoint there were **zero missing paths and zero
changed protected files** in the 0.17 tree. The archived 0.16 current product
review and validation documents matched the prior current bytes exactly.

The same independent comparison found `scripts/verify.py` unchanged from the
accepted 0.16 archive. Its **43 gate declarations**, including commands, were
identical. The helper additionally requires the current declared gate list in
order and the full inherited set of gate names. Counts are derived from hashed
gate logs and checked against each gate record, the distinct-test manifest,
per-crate distinct counts and final release totals. Zero failed/ignored tests,
successful exit codes, completed source/evidence flags and nonempty successful
acceptance checks remain mandatory.

## Checks reviewed

| Area | Finding |
|---|---|
| Baseline and inherited originals | The accepted ZIP is pinned by digest and checked for canonical regular members, duplicates, CRC and bounded size/count. Derived original/history maps absorb the inherited maps and recheck every inherited digest against the exact accepted ZIP. Current packaging requires every baseline path and every protected digest. |
| Current source freeze | The assembler reproduces the gate runner's implementation-input scope and requires exact map equality, including newly added or removed inputs. Repeated complete snapshots and per-open inode/metadata/content checks reject changes while acceptance records, manifest and ZIP are read or written. |
| Evidence references | Gate, native, desktop, release and preservation paths/digests are bound to one final version and gate record. The actual native executable and desktop archive are rehashed, toolchain/version are checked, and nested path/digest references are revalidated. |
| No replacement | Existing manifest, source ZIP, external record and preservation comparison are refused. Publication uses synchronized temporary files and exclusive hard links. Failure cleanup removes only inodes published by that invocation. A process interruption can leave staging files and does not justify replacing an existing artifact. |
| Private runtime state | Saves, user-data/cache/build folders are excluded. New/changed database, save/profile/session/backup formats and private keys/capabilities are screened; known accepted synthetic fixtures, TLS test material and historical logs remain eligible only at their exact accepted hashes. Canonical paths, symlinks and case collisions are checked. |
| ZIP and manifest | Every expected file, ordering, size, SHA-256 and Unix mode is checked after creation, as are CRC and exact embedded-manifest coverage. The external record sits outside the ZIP to avoid a circular hash. |
| Preservation publication | It checks a point-in-time inventory and exact retained content; it does not purport to run final gates or native/desktop acceptance. Final assembly rechecks preservation and source freeze independently after later release records are added. |

Both helper sources passed Python AST syntax parsing with bytecode creation
disabled. Reviewed helper hashes:

| Helper | SHA-256 |
|---|---|
| `package_source_017.py` | `fed16cfd5447e63fe1b1a43b1bf29cd3745f3436500ffe21d5f1861bdf9f4cb3` |
| `release_preservation_017.py` | `6b39c095145f63d0f5d06fee2bd4703ee728081d6d949bbdeaab21c6332988fc` |

This is code review and read-only baseline verification, not a completed source
release, desktop qualification or publication receipt. The source tree must be
quiescent and the final native, automated, desktop and release records complete
before the root invokes assembly. Local integrity and format screening do not
authenticate a publisher or protect against an administrator replacing paths
concurrently; existing trusted-local-path scope remains.

## Diagnostic-document cross-check

`PLAYTESTING.md` and `REVIEW_GAPS_v0.17.md` correctly describe opt-in/local-only
capture, the fixed per-user-data path, 10-second cadence, bounded retention,
file-manager collection without a folder-opening/clipboard claim, OS writer
ownership, private payload exclusions, and stale/exit/checkpoint distinctions.

Three concrete copy corrections were sent to the root, which owns those files:

1. The gap report should distinguish executed capability/typed-text sentinel and
   inventory/player-key assertions from source-reviewed absence of raw-log and
   balance serialization. The existing regression does not seed/assert every
   category named in that paragraph.
2. Describe initial capture as **attempted during the first running update**;
   startup can fail before any current-session report exists.
3. Playtesting step 9 should name the actual **Save and return to main menu**
   action.

Later edits and final acceptance claims must be checked against the final
records. This review does not promote the earlier 179-test implementation run
into native or release acceptance.
