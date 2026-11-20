# Independent domain and evidence documentation review — 0.24

Reviewed on 30 October 2026 after domain/native implementation freeze. The review
read `PRODUCT_REVIEW.md`, `RESEARCH_v0.24.md`, `REVIEW_GAPS_v0.24.md`,
`IMPLEMENTATION_PLAN_v0.24.md`, `ARCHITECTURE.md`, `SETTLEMENTS_AND_ECONOMY.md`
and `GUIDANCE.md`, with their current compatibility/validation statements and the
retained focused receipts. No domain source or current product document was
edited by this reviewer; exact corrections were sent to the integrator.

## Corrections identified and inspected

1. **Refund-policy contradiction.** The older supply-readiness paragraph still
   said that no new refund policy was introduced and issuer escrow remained on
   the board. It now distinguishes Return itself from later public maintenance:
   Return releases the claim without spending goods or paying a reward;
   confirmed obsolete public work may retire only after exact cap-safe credit to
   its original issuer. General withdrawals and treasury refunds remain absent.
2. **Expired work does not always remain on the board.** Older economy,
   architecture and guidance paragraphs described unconditional reopening of
   expired accepted work. They now describe claim release followed by public
   demand evaluation: needed offers renew under the same ID; confirmed obsolete
   offers may retire with safe issuer credit; unknown demand or refund failure
   retains escrow. These corrections agree with the appended 0.24 policy.
3. **Offline deadline scope.** Architecture described dormant owners as receiving
   no job-deadline advancement without distinguishing job kinds. It now scopes
   this to personal road jobs and separately states that supply deadlines follow
   persisted world time. This matches `transport/simulation.rs` and
   `settlement/economy.rs`; it does not introduce offline wall-time simulation.
4. **Focused status lag.** Validation still described only the initial 13-test
   supply harness and fixture corrections as ongoing. The plan had an empty
   execution ledger. The integrator updated both from retained outcomes: 83
   sandbox tests, 250 native tests, 53 persistence tests and 72 Python tests.
   These are separate focused runs, not an additive release-wide total. Final
   real-window/canonical/delivery acceptance remains explicitly pending.

The source-provenance wording was also corrected and inspected: sandbox and
native receipts contain unchanged input maps; the reliability focused receipts
retain command/log/bounded-execution evidence without their own source maps.
The current plan and validation page make this distinction explicitly. Final
canonical acceptance supplies the complete implementation boundary.

## Policy and compatibility assessment

The reviewed current policy distinguishes new posting from existing commitments.
Only new offers are sized to useful fitting batches of 1 through 12. Temporary
capacity loss or partial positive demand does not rewrite an existing public
offer. Accepted, unexpired terms remain honored; expiry and explicit return keep
their existing role. Missing/replaced receiving data is unknown, and missing or
capped issuers retain value. Retirement conserves the exact original escrow and
does not reuse identities or replenish money.

The lazy implementation description is accurate: shared demand classification
does not clone receiver inventories, while posting alone searches bounded fitting
batches. `ObsoleteOffer` is a public Rust enum addition requiring downstream
exhaustive-match updates; it adds no serialized command or saved-state field.
Source constants still report wire protocol 11, domain save 1 and regional
database schema 1. The research correctly distinguishes matching wire numbers
from arbitrary mixed-release qualification.

Native documentation matches stable reviewed operands, explicit desired
production state, exact missing-selection recovery and one-use named damage
confirmation. Continuous timers/payroll/rental countdown are excluded from
identity while current authoritative eligibility still applies.

All G01–G18 IDs and area names match the delivered 0.23 register. Selected changes
are scoped improvements rather than closure of whole product areas. Research
patterns are distinguished from Terra policy and execution proof. Windows,
physical devices/GPU, 200% reflow, uncoached careers, public-service operations and
distributed capacity remain explicit open requirements.

## Evidence boundary

This was a read-only documentation/source/receipt audit. No Cargo invocation,
new native journey, final acceptance or archive qualification occurred during
it. The corrected material policy statements were inspected after the integrator
applied them. Current documents remain active integration inputs until the
integrator's final documentation and preservation freeze.
