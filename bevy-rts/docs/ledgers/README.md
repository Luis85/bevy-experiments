# Build ledgers (snapshot)

These are the subagent-driven-development ledgers for each plan run: the controller's `progress.md` (every
ruling `R<n>`, task outcomes, review rounds and pause/resume blocks), the task briefs and reports, reviewer and
implementer instructions, and the architect/planner reports.

The live working copies are in `.superpowers/sdd/`, which is git-ignored because the skill writes scratch files and
review diffs there. This folder is a committed snapshot, so the decision history survives a fresh checkout. Review
diffs are left out; they can be regenerated from git history.

| Folder | Plan |
|---|---|
| `2026-10-23-frontline-m0-foundation/` | `docs/superpowers/plans/2026-10-23-frontline-m0-foundation.md` (superseded by rev 2) |
| `2026-10-23-frontline-m0-rev2/` | `docs/superpowers/plans/2026-10-23-frontline-m0-rev2.md` (M0, tag `m0`; also the M1–M9 roadmap) |
| `2026-10-24-frontline-m1/` | `docs/superpowers/plans/2026-10-24-frontline-m1.md` (M1, tag `m1`) |
| `2026-10-25-frontline-m2-prelude/` | `docs/superpowers/plans/2026-10-25-frontline-m2-prelude.md` (in progress: Tasks 1–5 and 7 done) |

Rulings are numbered across all ledgers. The next free number is listed in `RESUME.md`.

**Refreshing the snapshot.** When you pause or finish a plan, copy the `*.md` files and `plan-path` from
`.superpowers/sdd/<plan>/` here and commit them with the plan's other artifacts.
