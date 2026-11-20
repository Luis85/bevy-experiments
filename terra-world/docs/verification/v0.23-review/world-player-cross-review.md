# Independent player-oriented review of world readiness and guidance

Reviewed on 30 October 2026 by the player-experience specialist after the
world specialist implemented WP03–04. This is a read-only independent review;
the reviewer did not edit the domain implementation. Focused execution is
reported by the integrator in current validation, not inferred from this review.

## Result

No blocking defect found in the reviewed readiness/preparation, capacity-aware
acquisition or stated recovery contracts. The native integration uses the shared
API for written blockers, disabled controls and pre-dispatch preparation. The
ordinary authoritative command still checks the current state again.

## Evidence and reasoning

| Boundary | Review finding |
|---|---|
| Readiness versus authority | `settlement/supply.rs` owns acceptance, return and exact delivery preparation. `settlement/commands.rs` invokes those same preparations. Delivery computes complete player inventory, receiver inventory and final wallet before applying assignments; failure cannot leave a partly credited reward or partly consumed load. |
| Identity, ownership and expiry | Readiness rejects missing/dormant principals, missing accounts, missing requests, another owner's accepted work and expired delivery/acceptance. Returning the caller's expired request stays possible. The native target record retains the full displayed request and owner/restore context. |
| Recipient capacity | Receiver stock is prepared using the ordinary bounded inventory exchange and the producer's aggregate capacity. A moved or mismatching destination does not inherit the request merely by retaining an ID. Two successful inspections reserve nothing; the second delivery rechecks capacity after the first. |
| Money and public escrow | Successful delivery moves the exact reserved reward into the rightful wallet, then removes the request. Returning marks work unclaimed and retains its escrow; no refund, new currency or synthetic treasury replenishment is introduced. Reward text remains gross rather than profit. |
| Partial and remote views | A missing service is described as out of the current view or unavailable, not globally deleted. Guidance can travel to the recorded position when outside reach and asks for another review after loading. The source API explicitly says replica readiness is not a reservation. |
| Recovery choice | Blocked held work names the concrete blocker and keeps an explicit Return action available. Return is labeled as spending no goods and paying no reward. It is not executed automatically when a view refreshes or a receiver is temporarily full. |
| Complete crafting exchanges | Acquisition checks all inputs and outputs together, including slots freed by consumed inputs and byproduct stack limits. It selects a smaller executable batch where useful. It does not create a second stock ledger or spend ingredients while deriving advice. |
| Bounded resource advice | Existing candidate-count and recursion limits remain. Gathering tries at most five quantities; crafting tries at most twenty batches. A reachable finished-item seller can remain an alternative to blocked crafting. Capacity failure points to inventory/selling rather than repeatedly advertising an impossible command. |
| Dormant guidance | The new top-level dormant-principal guard prevents every goal track from advertising action for a suspended owner. It complements native action-context refusal and authoritative rejection. |

## Test design reviewed

`tests/supply_readiness.rs` checks read-only inspection, exact value movement,
ownership and expiry refusal, receiver/wallet capacity, current-view absence,
two competing final-capacity deliveries, continuation through save/restore and
recovery through real spending. Tests compare complete state on rejection and
validate appropriate saved-world fixtures. Finite authored requests explicitly
reserve existing issuer cash instead of silently minting rewards.

`tests/guidance_capacity.rs` exercises multiple recipe outputs, byproduct stack
limits, a real quoted finished-item purchase, inputs that free slots, a smaller
affordable batch and near-full input acquisition followed by crafting. It
separates a catalog-valid 64-kind inventory from an intentionally invalid raw
SDK fixture; the latter is not called a valid saved-world case. Guidance
derivation is checked for absence of mutation, and executable recommendations
are passed through normal authority.

The native test additions independently exercise wallet-capacity text and
disabled state, expired acceptance, rejection before enqueue, and successful
delivery after the condition is resolved. Actual activation and queue identity
tests belong to the player implementation and the reliability specialist's
independent boundary review.

## Limits and follow-through

These checks establish bounded correctness and coherent explanation, not human
comprehension or long-career balance. The new return/capacity text still needs
the integrator's current native minimum-window inspection. Uncoached Supply,
Home/Community and Road/Company sessions, physical input devices, assistive
technology and localized text remain separate product acceptance. Receiver
availability and price comparisons can change after inspection; no screen or
API success is represented as a cross-session reservation.
