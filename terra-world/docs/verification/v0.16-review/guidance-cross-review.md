# Independent guidance cross-review — 0.16

Date: 2026-10-29 (Europe/Berlin). Reviewer: navigation increment subagent,
independent of the guidance implementation owner. This is a source/behavior
review; fresh native screenshots and integrated execution belong to the parent
acceptance run.

## Reviewed product and authority contracts

The commitments briefing is derived from the requested principal's accepted
supply, assigned jobs, company and owned rentals. It does not create a saved
progress ledger, income or a second reward/ownership authority. Supply rows are
bounded by the existing three-active limit and sort by deadline then stable ID.
Partial views are named. Rental bonds are distinguished from available cash.

An explicit accepted request retains priority; otherwise deadlines select the
next accepted request. The full SupplyContract snapshot participates in intent
equality, so two requests cannot share authority merely because both recommend
the same gather command. Principal and restore revisions protect UI handoffs.
The native action phase precedes world simulation/persistence and the later
guidance/label refresh, so the displayed snapshot remains the comparison for a
click rather than being silently replaced just before dispatch.

Price comparison uses all missing units and a seller stocking that complete
quantity, with checked multiplication and stable selection. It distinguishes
gross reward, purchase shortfall, personal wallet, unknown carried-goods basis
and unpriced travel. It does not reserve inventory or execute a trade. The
ordinary quote, inventory/cash-capacity, reach and server command rules remain
authoritative. Expired accepted requests expose value-preserving return; an
ordinary supply expiry can also unclaim a request before the next rendered
snapshot, which the stale guard rejects.

## Finding and applied polish

**G04-R1 — A queued selection change could retarget a read-only review.**
ReviewAcceptedSupply calculates its next request from cached commitments and
the current noticeboard selection. NextContract changed that selection but
invalidated only guidance context. If selection and review actions arrived
before the next render, the review could therefore choose a different accepted
request from the last displayed label. Authority commands and money were not
affected, but the promised exact review handoff was incorrect.

Coordinated with the guidance owner, NextContract now also invalidates
commitments_context. A second queued action waits for the new display. The
regression `queued_selection_and_review_actions_wait_for_the_new_displayed_handoff`
also checks the reverse sequence: opening a review, then attempting cancellation
before refresh, including a single accepted request whose snapshot remains
equal. Neither sequence retargets, queues movement, mutates the world or emits a
transaction.

The targeted native test attempt was blocked during compilation by an unrelated
PlayerControl private-field construction in remote.rs. The owning root run is
resolving that integration issue and will execute the complete terra-demo test
suite, including this regression. This note does not label a blocked compilation
as a passed test.

## Verification status update after final-polish fixtures

The preceding paragraph preserves the status at the original source-review stage.
It is separate from the retained first final-gate attempt in
[v0.16-attempt-1/gates.json](../v0.16-attempt-1/gates.json). That attempt passed
compatibility and format, then exited 101 in the headless compile: two new
guidance test fixtures called the nonexistent `LocalFrame::unproject` method.
The failed gate record and its headless log remain intact; it is not a passed test
run and is not replaced by the focused result below.

Only those two fixture conversions were corrected to the project's existing
`LocalFrame::to_geo(...).unwrap()` API. `cargo fmt --all` exited 0. The focused
command `cargo test -p terra-sandbox --test guidance_commitments --offline --locked
--jobs 3` then exited 0: **9 passed, 0 failed, 0 ignored, 0 measured and 0 filtered
out**, with 0.21 seconds of test execution after a 47.42-second build. Both new
market/recipe comparison tests and the sustained save/resume journey passed.
Production behavior was not changed by this compile correction.

The independent reviewer also read the final cost/scroll polish and requested no
additional source change. This focused result establishes execution of the domain
test target; it does not establish the full release gate set or corrected native
interaction. The parent has restarted the final gates. **Final release and native
acceptance remain pending.** This status update was recorded by the guidance
implementation owner; the independent source review and earlier failure history
above are retained.

## Remaining review scope

No additional economy, ownership or stale-transaction defect was found in the
reviewed change. Candidate chooser, commitments/cost, scroll and small-window
findings are recorded in [native-cross-review.md](native-cross-review.md).
Corrected final-native readback still belongs to the parent acceptance run. The existing report
correctly leaves uncoached human comprehension, longer career balance and broad
device/accessibility qualification open.

Reviewed code: [domain guidance](../../../crates/terra-sandbox/src/guidance.rs),
[commitments](../../../crates/terra-sandbox/src/guidance/commitments.rs),
[domain regression journey](../../../crates/terra-sandbox/tests/guidance_commitments.rs),
[native handoff](../../../crates/terra-demo/src/frontier.rs), and the existing
interface/interaction system ordering. See the owner's
[implementation record](guidance-implementation.md) for its separate evidence.
