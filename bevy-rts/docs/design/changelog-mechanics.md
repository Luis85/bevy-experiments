# Mechanic change requests (MCR)

Every change that alters game rules or formulas, or that changes the golden replays, is a **mechanic change request**
(ADR-0021, spec §9.2 C8). Its entry is written here **before the code**, so the prediction is on record before any
replay is re-recorded. The `Change-Class: F` commit that implements it carries the trailer `MCR: MCR-nnn`, and the
golden replays it changes are re-blessed only through `fl_cli rebless --reason MCR-nnn` (R146), which refuses an MCR
id that has no heading here.

## Entry format

Entries are numbered `MCR-000`, `MCR-001`, … in the order they are written, never reused. Each is a level-2 heading
followed by these six lines:

```markdown
## MCR-nnn — <short title> (<plan items or rulings>)

- Why: <the problem or the decision that needs the change>.
- Requirements: <rule ids added, changed or removed (`PATH.QUEUE.11` new, `FLOW.GROUP.7` changed), or none>.
- Predicted effect: <what changes in the golden hash streams (first divergent tick, or none), poses, journal records
  and counters, and what stays identical>.
- SIM_VERSION: <old> → <new>, or unchanged (<n>).
- Change-Class: <R|D|F|N|E|X>.
- Re-bless: <filled in after the run: the replays re-blessed, the `fl_cli rebless` report's verdict against the
  prediction, and the ratification list it is on (Task 21); or none>.
```

Rules for the entries:

- **Prediction first.** The predicted effect is written before `fl_cli rebless` runs. A difference the report shows
  but the entry did not predict is a finding to explain or fix, not something to re-bless.
- **One bump per task.** A class F task bumps `SIM_VERSION` at most once; class R and E changes keep every golden
  hash stream bit-identical and need no MCR unless they change the replay format (as MCR-000 does).
- **Specs move with the MCR.** An MCR that changes a requirement updates the mechanic spec in the same task (a new
  `spec-version` and a `## Changelog` line naming the MCR), written as `.md.new` and blessed by the lead.
- **Ratification.** Every re-bless done under an MCR is listed in the milestone's final report for the user's
  ratification (R146).

## Entries

The M2 prelude adds MCR-000 (Task 5, replay format 4), MCR-001 (Task 13, order queue and activity stack) and MCR-002
(Task 15, path lanes, flow stall rule, reload invalidation).

## MCR-000 — Replay format 4 (C8, M10)

- Why: golden replays need versioned scale params (a new `ScaleScenario` field no longer bumps the replay format),
  named semantic checks next to the hashes, and a journal summary for re-bless reports (ADR-0021).
- Requirements: none.
- Predicted effect: replay bytes change (format 3 → 4, the record functions' semantic checks, a journal summary); hash
  streams identical (first divergent tick: none); poses, journal records and counters unchanged.
- SIM_VERSION: unchanged (2).
- Change-Class: R.
- Re-bless: done by the lead (R146, R156) in the replay-format-4 commit: `testbed_wander.replay` (400 ticks) and
  `testbed_paths.replay` (900 ticks) re-recorded with their semantic checks; `fl_cli rebless` verdict: hash streams
  identical, header and format only. Listed for the user's ratification in Task 21.
