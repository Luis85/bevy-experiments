# Continuing guidance implementation and review — 0.16

Date: 2026-10-29 (Europe/Berlin). Owner: guidance increment subagent.
Status: implementation and targeted domain verification complete; integrated
native and release acceptance remain the parent run's responsibility.

## Reviewed gaps

The current code exposed six goal tracks through a cycling control, provided no
compact inventory of accepted obligations on returning, and defaulted to the
lowest accepted supply ID. The guide described funded rewards without a complete
missing-input purchase comparison. Its action-only stale check could also treat
two different requests as identical when both proposed the same gather command.
The ordinary supply noticeboard could fall back to another request between
display and dispatch. These are source findings, not invented player-study data.

## Implemented behavior

- Activities offers six direct choices through an expandable goal chooser.
  Suggested retains live needs/obligation prioritization; changing goals does
  not cancel work. The choice is session UI state, not a new saved quest ledger.
- A bounded owner-only commitments briefing shows accepted supply, personal
  wallet, road assignments, company cash/loaded commitments and rental bonds.
  Supply rows follow earliest world-clock expiry and stable request ID, capped
  at the existing three-active-request limit. Partial received views are named.
- An explicitly selected accepted request retains priority. Otherwise Supply
  and the noticeboard use earliest expiry. Review supply request names an exact
  retained request, opens that request and waits for a separate next-step action.
- Full request snapshots plus principal/restore context protect both guided
  actions and noticeboard transactions/travel. A changed identity, reward,
  deadline, quantity, owner or target refreshes without substituting another
  transaction. Read-only review does not enqueue movement or authority commands.
- Supply distinguishes gross reserved reward from input costs. It compares all
  missing units with the cheapest complete visible stock, names purchase/reward
  shortfalls and insufficient wallet, and explicitly leaves carried supplies and
  travel unpriced. It does not reserve stock or promise profit; ordinary Market
  remains the transaction boundary.
- Deadlines name world hours, minutes or seconds, with singular units and a
  distinct expired state. An expired accepted request offers an explicit return
  without goods consumption or reward. Impossible expired delivery is disabled.

The SDK API adds `activity_commitments`, `ActivityGuidance::same_intent` and
`supply_deadline_label`. None is serialized into world saves or network messages.
The [feature guide](../../GUIDANCE.md) describes contracts and limitations.

## Verification

During implementation, the existing 23 guidance tests and 9 company-guidance
tests passed. The final targeted `guidance_commitments` run passed all **7 tests**
with zero failures/ignored cases. It covers owner filtering, bounded rows,
save/resume, expiry priority/explicit selection, same-command/different-obligation
staleness, full-quantity price comparisons, deadline text and expired return.

The continuity report emitted by
`continuing_guidance_journey_preserves_obligations_and_rebuilds_after_resume`
records:

| Measure | Observed value |
|---|---:|
| Accepted supply requests | 3 |
| Explicit returns | 1 |
| Missed deadlines | 1 |
| Paid deliveries | 1 |
| Calendar seconds advanced | 86,401 |
| Physical walking seconds | 70.8 |
| Walking distance | 315.9 m |
| Maximum admitted walking step | 0.4500000022 m |
| Delivered stock | 4 wood |
| Personal wallet, before → after | 250.00 → 255.68 credits |
| Gross reserved reward | 5.68 credits |
| Community XP earned | 40 |
| Remaining accepted supply | 0 |

All twelve report verification flags passed: ordering; exact open obligations
after resume; value-preserving return; duplicate-return rejection; ordinary
deadline return to board; stale-obligation review; preservation of the remaining
request; physical delivery without a position override; exact stock/payment;
duplicate-payment rejection; exact completed save/resume; complete catalog
validation. The fixture uses the existing minimum 24-world-hour contract duration
and separately advances calendar and physical motion on authored flat SDK support.
It is not prepared-terrain coverage, career profit, hardware or human-comprehension
evidence.

Six new native integration tests cover direct choice and actual chooser node
visibility, exact read-only request review, cancelled/reassigned/restored context,
noticeboard retarget rejection, same-gather/different-obligation rejection, and
queued selection/review handoffs before labels refresh. The last was added by
the independent navigation reviewer after finding that NextContract also needed
to invalidate the cached commitments context; see [the cross-review](guidance-cross-review.md).
Their execution belongs to the integrated terra-demo pass. The offline navigation
allowlist in `interface.rs` admits only the new read-only goal/review controls;
transactional actions retain the existing disconnected-client block.

## Native review handoff

1. Open Activities in a new world and expand Goal → choose. Inspect all six
   choices and close/select each while retaining a visible next action.
2. Accept two requests using the noticeboard or the guided action. Inspect the
   commitments rows and use the explicitly named review button to alternate
   accepted requests. Verify that review alone does not move, consume or pay.
3. Inspect a missing-input request: gross reward, full missing stock comparison,
   deadline clock and recovery alternatives should be readable through scrolling.
4. Save, reopen and inspect Suggested/commitments. Existing obligations should
   reconstruct; the prior session's explicit goal selector is not persisted.
5. Repeat the interaction at 1100 × 700 with 150% text, covering the expanded
   chooser, primary action, commitments, noticeboard and bottom actions. Verify
   keyboard focus and scroll reachability using fresh native captures.

## Cross-review and remaining scope

### Native review and final polish batch

The independent review used the parent's fresh native candidate captures at
1440 × 900 and 1100 × 700 with 150% text. The capture-level observations and their
limits are recorded in [native-cross-review.md](native-cross-review.md). This was
an instrumented developer walkthrough, not uncoached player research.

Two observed guidance defects drove the final batch: reviewing an accepted request
left its new action above the current scroll position, and the missing-wheat step
repeated market prices/wallet while omitting its gross reward. Explicit goal
selection now uses the same reveal behavior because its lower choices require
scrolling in the small-window layout. Successful explicit review/selection sets a
clear status and resets only the inspector; automatic or stale refresh does not.

The supply brief now retains gross reward and XP through acceptance, acquisition
and ready-to-travel steps, with one full-quantity input-price comparison. Actual
acquisition seller/item, recipe prerequisites and affordability/capacity blockers
remain separate from that comparison. The independent navigation reviewer checked
this distinction: a complete stock/price comparison does not establish that the
whole exchange passes reach, inventory or market-cash capacity. Live Market quotes
remain the transaction boundary. Unavailable-comparison copy also stays true when
checked price multiplication cannot produce a representable total.

Two additional domain regressions exercise different action/comparison sellers,
real quote arithmetic and capacity, shortfall visibility, and ingredient versus
finished-goods identity even when seller names match. Existing goal/request tests
now assert reveal/status and unchanged scroll revisions after stale or automatic
refresh. One new interface regression exercises the real inspector scroll reset
while preserving the other panel and later refreshed/text-scaled reading position.
Only owned files were formatted. No Cargo or display process was started for this
batch; compilation, execution and fresh post-polish native captures belong to the
parent's final integrated gates. The earlier passing counts above describe their
original run and must not be read as execution of these later assertions.

An independent source review of the parent's recovery naming work is recorded in
[recovery-cross-review.md](recovery-cross-review.md). Its small polish correction
makes restore progress name the chosen recovered title before the original backup
and checkpoint, with a queued-action-to-worker result assertion.

This increment improves G03/G04 decisions and continuity, plus stale-context
reliability. It does not close uncoached first/returning-player studies, long career
balance, an authored quest library, complete controller/accessibility qualification
or general economic cost-basis reporting. Final native observations and all
release counts must come from the parent acceptance record.
