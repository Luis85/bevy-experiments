# Direct native confirmation provenance

**Status: direct native confirmation and terminal canonical association passed.**
This report describes the directly recorded final confirmation. It
does not promote the earlier candidate child process to a verified final-binary
run.

The [accepted audit](confirmation-provenance-accepted.json) reread the actual
successful terminal receipt, all **44 gate logs**, **36 accepted artifacts**,
the frozen implementation and native executable. The receipt records **1,070
distinct Rust tests** and **1,171 Rust test executions**; its separately retained
Python gate log reports **50 tests passed**. No failures, ignored tests or
timeouts were recorded. The completed canonical run spans **08:34:46–09:05:55 UTC** on
30 October 2026, after both direct confirmation processes had finished.

The exact terminal receipt SHA-256 is:

`12b4a3f72591c7431bbc0aeda6fe189c5650b81da29d29c7fa8ef955c903cfa0`

The first successful native-only audit remains retained as
[confirmation-provenance-pending.json](confirmation-provenance-pending.json).
Its status is deliberately `pending_canonical_acceptance`; its
`terminal_receipt_validated` field remains false. The later accepted report was
published separately, after the actual completed receipt existed; the earlier
observation was not rewritten.

## Direct process and capture chain

The [validator](audit_confirmation_023.py) independently read the following
recorded processes and all ten associated capture pairs:

| Process | PID | Captures | Recorded interval on 30 October 2026, UTC | Exit |
| --- | ---: | --- | --- | ---: |
| [Recovered world](confirmation-recovered-process.json) | 14073 | 70–76 | 08:29:04–08:32:32 | 0 |
| [Main menu](confirmation-menu-process.json) | 14392 | 80–82 | 08:33:08–08:34:20 | 0 |

The start and terminal records agree on their direct command, executable hash,
PID, source identity and working directory. Neither process was wrapped in
`strace` or treated as an expected interruption. The recovered-world command
opens its recorded `--world-profile` directly; the menu command uses `--menu`.

Every capture's before/after window PID equals its corresponding recorded
process PID. Its timestamps lie inside that process's own lifetime. The images
are valid PNGs whose exact names, byte lengths, SHA-256 values and dimensions
match the sidecars. The final set contains four 1440×900 captures and six
1100×700 captures. Both retained process logs match their terminal-record hashes.

The directly opened recovered world displays the continued local state:
181.13 cr, 50 XP, one completed supply contract and Review23's 75.00 cr treasury.
The small gameplay viewport, activity/territory panels, pause/settings controls
and saved exit are retained. Menu and session settings show the corrected
effective atmospheric-sky state under Performance while preserving the user's
separate sky preference. These visible observations are described alongside the
screenshots in the [native review](README.md).

## Frozen implementation and executable

Both processes record **381 implementation inputs** with identical canonical map
digests at launch and finish:

`dd6d40911a1defed04d39a3a4287ae01ced1cfde27fdcbf0575caa404c39c5d5`

The validator recomputed that digest from the current complete `source_inputs()`
map and compared the map itself with the canonical run's already published
`source-hashes.json`. The digest uses sorted keys, compact JSON separators,
ASCII escaping and disallows nonfinite numbers; it is distinct from hashing the
indented source-manifest file's raw bytes. Both identities are retained in the
machine-readable audit.

The current executable was independently reread and matched both process
records:

`432748a47f507d23e3efdbd2862f4e5beaae68c41801eeaf4a057392d4f3c99f`

The audit rereads implementation inputs, binary bytes and every observed native
evidence file before publishing its result. It does not substitute a newer
executable at the same pathname or infer unchanged inputs from an input count.

## Terminal acceptance boundary

The original pending check used only the canonical run's frozen source map. It
did not treat a running progress record or a count of completed gates as
acceptance.

The successful `--phase accepted` run additionally checked:

1. The real successful 0.23.0 `completed-gates.json`, with an identical
   `gates.json` alias and all unchanged-source/report/log flags true.
2. The exact ordered **44-gate** canonical contract, integer successful outcomes,
   no failures or ignored tests, explicit no-timeout results and correct passing
   Rust execution totals.
3. The pinned compiler, successful bound metadata/toolchain probes, unique
   crate/harness/test identities and matching per-crate and distinct totals.
4. Actual bytes matching every accepted artifact and gate-log digest, including
   the frozen source map, plus the same directly confirmed native executable.
5. A fresh canonical execution starting after the direct confirmation finished,
   followed by another complete reread of the native and accepted evidence.

An absent, failed or inconsistent terminal receipt aborts accepted-mode
publication. The accepted run matched the current source map and executable to
the real receipt, then rehashed the observed native and canonical files before
publishing its passed result. The retained pending report was preserved.

The actual accepted-mode refusal was exercised while the canonical run was
still in progress. It returned exit **1** for the absent terminal receipt and
created no accepted report. The exact command, helper hash and observed error
are retained in [confirmation-admission-refusal.json](confirmation-admission-refusal.json).
This is negative admission evidence, not a failed native confirmation or a
successful canonical run.

## Limits retained

The [candidate audit](candidate-evidence-audit.md) still has 23 gameplay captures
with stable window PID 12147 but no independently retained child process,
executable or exit association. The final confirmation supplies a new direct
chain for the recovered continuation and menu. It does not invent the missing
historical association or claim that every earlier candidate action was replayed
on the final executable.

The [saved-state inspection](state-inspection.md) separately verifies original
and backup equality and the recovered continuation's persisted invariants. A
successful native process exit alone is not that durable-state proof.

The observed renderer is Vulkan llvmpipe on Linux/X11. These observations do not
qualify Windows, physical GPUs, controller/wheel hardware, assistive technology,
uncoached players, publisher authentication or power-loss durability.
