# Final review and bounded fix pass

The source review covered the resumed plan from robot presentation/editor through registration, worlds and
landmarks, and swept the historical deferred findings. F changed documentation only; its post-gate refresh
retained the same source findings. This is one bounded fix pass, with diagnostic/test corrections as needed.

## Confirmed regressions

Before production changes, the targeted nextest run (`target/final-fixes-red.log`) ran 17 tests: 15 failed and
two control cases passed. A separate kernel bound test also failed with the signed-negation overflow. Tokens
were not printed by the redaction failure messages.

| Finding | Baseline evidence | Resolution/verification |
|---|---|---|
| Craft matching-stack capacity | Separate output appended to a full two-slot pack | Regression passes; no merge/ID allocation change |
| Craft feedback | No inventory_gained; dropped field/text absent | Actual craft/delta, drop positions and legacy decoding pass |
| Opaque stamp ABA | Old token accepted after tick/date rollback and reopen | Reopen/rollback/stale/exhaustion regressions pass |
| Secret Debug holders | Four actual CLI/server holders exposed sentinels | Four holder/nested-CLI regressions pass; parsing unchanged |
| Carried/equipped visuals | Real pickup leaves ground model; equip/startup/ghost fail | Five renderer regressions and six native captures pass |
| Position extremes | MIN panics; malformed agent Move submits no safe refusal | Closed-boundary/extreme and atomic agent rejection pass |

The equipped-slot and journal-enabled/disabled state-parity control tests passed before the fixes and must
remain green. Valid-state replays/snapshots and numerical budgets remain protected.

## Historical deferred items

- Long-frame act-and-wait tombstone lookup and cross-process MCP nonce uniqueness were corrected in I.
- File ID/revision exhaustion, byte identities and guarded rename/migration cleanup were addressed in H;
  the final pass closes the additional clock-reset version repetition.
- F7's consumed release/stuck marquee was corrected and tested in H.
- The strict hunger/thirst collapse rule is documented R22/ADR 0017: external rescue is required. It was
  reviewed and withdrawn as a new defect; no undocumented recovery policy is introduced.
- Remaining low-impact historical notes (cosmetic wording, duplicate clipboard plumbing, unsupported path
  encodings, stress-only cache retention and screenshot niceties) do not block the specified workflows.

## Final evidence

The expanded nextest run (`target/final-fixes-green.log`) passed 286/288 tests. Its only failures were two
old Notes assertions expecting revision1 after mutation. Their exact error/file-byte expectations now include
revision2, with all unknown-property and stale-write assertions retained; both focused reruns passed
(`target/final-fixes-notes-green.log`). Workspace/all-target check and Clippy with warnings denied passed.
Cross-author patch reviews found no introduced Critical/Important defects. No dependency or oracle changed.

Actual current-debug native item probe `target/final-items-visual-394b68a395db/summary.json` reports `ok:true`.
Human and Rover each used the generated MCP bridge, real pickup/equip, a save with carried_by/main_hand and
no ground position, same-port native resume, unequip and drop. BRP verified exact `handslot.r` / `robot right
grip` parents, hidden unparented inventory and visible unparented ground. Six equipped/inventory/dropped
PNGs under that fixture were inspected by the worker and controller; all owned children exited. No OS input.

Final full `cargo xtask verify` passed: `target/verify-final.json` reports `ok:true`, 1,242 tests passed,
12 skipped and two reported leaky; doc tests 1 passed/10 ignored; architecture 7/7; CLI replays 8/8;
six snapshots with none pending; fmt/Clippy/deny passed, no optional tools skipped. All new schedule and
integrity regressions passed in that complete run. Final release artifacts follow before overall completion.
ADR 0019 records the choices; no oracle is rebaselined. Performance results below are separate from
correctness verification.

## Performance follow-up

Fresh final-source release harness `explorer_perf-ab1ffaf577565073.exe` was compiled with the existing
ignored benchmark, then run directly after all builders/runners/owned apps finished. The first five-round
measurement (`target/final-rover-perf.log`) reported moving Rover median p95 51.9 µs perception/66.5 µs fog;
a quiet repeat (`target/final-rover-perf-repeat.log`) reported 50.7/63.3 µs. Both budgets remain 55 µs.
The benchmark's ordinary test pass is not treated as budget compliance. Private solid-window bitmap reuse
and combined SOLID/VISITED flags retain every exact beam traversal and coverage cell. All 69 perception
tests passed (three existing ignored performance tests); the original 9,000-case marcher oracle remains
intact. Added tests exercise subcell reuse, invalidation, snapshot replacement and range/fog scratch reuse.

The final runtime also omits empty survival/night systems for Plain and disabled-night worlds. Enabled
mechanics retain the same systems, clocks and snapshot registration. Its new schedule regression initially
depended on feature-disabled Bevy debug names; it now compares concrete system type sets, preserving all
state, clock and sensor assertions.

Every fresh measurement is retained, including slower results: `final-rover-perf-cached.log` (56.5/76.5 µs),
`final-rover-perf-flags.log` (53.4/70.7 µs) and `final-rover-perf-runtime.log` (48.9/66.0 µs), all under
`target/`. These are moving Rover median p95 perception/fog. Independent GetSystemTimes samples taken
with no owned benchmark, builder or app running reported 91.1% host CPU before the flags follow-up and
67.2% immediately before the final runtime run. Unowned antivirus/Node/browser work was left running.
Human control cases also slowed substantially, so these runs cannot isolate the change's performance.

Those loaded-host runs do not establish budget compliance. The earlier Task 13 report of 37.2/49.4 µs
has no matching retained final raw log and is not used as final-source compliance evidence.

**Final quiet-host verification passed twice.** After the full gate, with no owned builder, test runner or
app active, independent idle-window GetSystemTimes samples reported 14.6% then 12.7% host CPU. Two
consecutive runs of the unchanged release harness (300 warm-up ticks, 3,000 measured ticks, five interleaved
rounds) measured moving Rover median p95 perception/fog at **34.3/41.9 µs** and **34.0/40.7 µs**. Every
reported case was within its unchanged 55 µs budgets. Raw logs: `target/final-rover-perf-post-gate.log` and
`target/final-rover-perf-post-gate-repeat.log`. Full scans and cadence remain unchanged. These are measured
quiet-host results, not latency guarantees under arbitrary external load; all earlier slower logs remain.
