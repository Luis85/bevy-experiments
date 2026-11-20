# 0.23 world and framework review

Reviewed 30 October 2026 against the exact delivered 0.22.0 source. This report
owns domain progression, inventory and money, communities, simulation boundaries,
authoritative multiplayer behavior and reusable framework seams. The integrator
owns current native screenshots and the canonical execution record. Prior
release evidence is context, not fresh 0.23 acceptance.

## Assessment

The framework already connects real geographic source identity, bounded regional
working sets, actor ownership, physical arrival, finite wallets, funded supply
requests, communities and transport commitments. It is most useful to improve
the complete player decision through command admission and recovery. A new
simulation clock, currency or distributed architecture would not resolve the
observed workflow defects.

The strongest source finding was a mismatch between advertised supply delivery
and authoritative capacity checks. A player could carry the requested goods and
reach the service, receive **Deliver and collect reward**, then get a generic
capacity rejection because the receiver or wallet was full. Acquisition guidance
also checked recipe inputs without preparing every resulting output, so a
valid custom recipe could propose an impossible multi-output craft. Authority
already preserved state; the missing part was truthful preparation and recovery
before the player acted.

## Coverage and findings

| Perspective | Evidence and finding | Disposition |
|---|---|---|
| Supply progression | `guidance.rs::supply_contract` used goods and reach to offer completion; `settlement/commands.rs` additionally checked receiving stock and wallet. Native Frontier duplicated the smaller predicate. | WP03 shares typed readiness with authoritative preparation and native integration. |
| Returning-player recovery | A blocked accepted request retained escrow correctly, but its guidance provided no actionable capacity explanation. | Preserve the exact request while explaining the blocker and an explicit value-preserving return. |
| Acquisition and custom catalogs | Ready recipe inputs did not establish output capacity; all recipe outputs and consumed-input removal must be considered. | WP04 prepares the complete exchange, chooses a feasible bounded batch, or offers a real alternative/recovery. |
| Expiry and current offers | Unclaimed expired requests remained eligible for the guide until maintenance, while authoritative acceptance rejected them. | Filter expired public offers; accepted expired work keeps its ordinary return action. |
| Economy and value | Reward escrow, finite payer balances, checked inventory exchange and candidate transactions already provide strong conservation. | Reuse them. No funding, refunds, new reward policy or stock refill is introduced. |
| Multiplayer and partial state | Recipient views can omit distant facilities. A successful local readiness observation cannot reserve future authority. | Explicit absent-view description; authority prepares again at execution. Two observations cannot share the same final receiving capacity. |
| Community governance | Safe sole-leader unused-faction dissolution exists. Populated/asset-owning dissolution needs property, treasury and succession policy. | Preserve its boundary. Correct a stale crate README that still described every dissolution as future work. |
| Long-career economy | Unclaimed requests reopen indefinitely after expiry, even when original shortages have changed. This can retain board slots and escrow. | Future policy design: cap-safe issuer refunds and withdrawal rules must distinguish accepted obligations. No implicit cancellation is added. |
| Living simulation and navigation | Existing fixed-step authority, accepted movement, dormancy and physical cargo arrival are established. | Preserve clocks, rates, positions and handling; broaden workloads only for demonstrated defects. |
| Framework extension | Pure-domain rules remain separate from Bevy rendering. New UI checks should not duplicate transaction policy. | Add a public typed query and private prepared mutation bundle with unchanged serialized command/save schema. |
| Qualification | Automated finite journeys cannot establish uncoached comprehension, long-term balance, Windows/device performance or Internet capacity. | Retain the corresponding G01–G18 gaps and explicit next proof. |

## Research applied

Microsoft's current PlayFab guidance distinguishes duplicate-request prevention
from state preconditions: a logical retry keeps its original identity and body,
while optimistic concurrency detects whether the state being modified has
changed. This supports preserving Terra's command-sequence discipline and
rechecking economic conditions at execution. Inspection remains read-only and
cannot stand in for authoritative admission. No PlayFab dependency is added.

- [Idempotent transactions and retries](https://learn.microsoft.com/en-us/xbox/playfab/economy-monetization/economy-v2/tutorials/idempotent-transactions-and-retries)
- [ETags and concurrency control](https://learn.microsoft.com/en-us/xbox/playfab/economy-monetization/economy-v2/tutorials/etags-and-concurrency-control)
- [Execute inventory operations](https://learn.microsoft.com/en-us/rest/api/playfab/economy/inventory/execute-inventory-operations?view=playfab-rest)

The official Bevy example separates fixed-step physical state from visual
interpolation and camera scheduling. Its relevance here is a constraint: none
of these readiness or guidance changes needs to change simulation time or
introduce a second economic clock.

- [Bevy: physics in a fixed timestep](https://bevy.org/examples-webgpu/movement/physics-in-fixed-timestep/)

## WP03: one preparation contract

New `settlement/supply.rs` exports `SupplyContractAction::{Accept, Deliver,
Return}` and `SupplyContractBlocker`. The public
`SandboxState::supply_contract_readiness` returns `Result<(),
SupplyContractBlocker>` and changes no state. Its explanations identify the
current session/request, accepted-work limit, expiry, carried goods, receiving
data, receiver capacity, reward-wallet capacity and reach.

Acceptance and return call their exact shared checks. Fulfillment uses a private
prepared bundle containing the named contract, debited player stock, credited
receiving stock and resulting wallet. It commits those values only after all
checks pass. Existing `SandboxError` categories remain at command boundaries.
The preparer also refuses a receiving service whose position no longer matches
the request's recorded target; validated worlds already require that identity.

The public query is not serialized, a reservation, a second ledger or a
cross-session capability. The normal command remains necessary. A changed
capacity, expired deadline or unavailable receiver can invalidate a previous
successful query. Missing data in a recipient view has deliberately qualified
wording. The guide preserves the recorded destination for travel and rechecks
after it loads.

When a held request cannot be delivered, guidance names the blocker and offers
**Return this supply request** explicitly. Keeping the request while resolving
the condition remains possible. Return consumes no goods and pays no reward;
the original funded escrow remains on the board. Expired unclaimed offers are
not advertised as acceptable before the actual bounded maintenance refresh.

## WP04: achievable inventory steps

The guide prepares crafting with the same inventory `exchange` rule used by
authority, trying at most 20 batches in descending order. Inputs are removed
before all outputs are credited. A smaller feasible batch can make progress;
the next live-state suggestion reflects what remains. Gather suggestions try
at most five quantities. The nearest-resource shortlist and three-level recipe
dependency bounds remain unchanged.

A capacity failure does not consume ingredients or resources. An available
finished-item seller can provide an alternative; otherwise the guide explains
that the player needs item-slot or stack space and opens the existing inventory/
market workflow. The actual command still validates reach, permission, source
stock and inventory when executed.

Catalogs themselves have a 64-kind cap, matching the inventory kind cap. A valid
catalog cannot contain a 65th distinct output kind. Tests therefore distinguish
valid full-catalog and stack-boundary scenarios from an explicitly adversarial
raw SDK inventory containing unknown kinds. The latter is rejected by full
catalog validation and is never presented as a valid saved-world fixture.

## Meaningful acceptance fixtures

`tests/supply_readiness.rs` contains nine focused tests:

1. Read-only inspection and exact funded fulfillment once, including replay
   rejection and unchanged goods/money totals.
2. Session/identity, expiry, goods, reach, receiver/wallet capacity, absent data,
   mismatched destination and catalog blockers with complete unchanged state.
3. Shared acceptance/return limits and intact expired-offer escrow.
4. Public expiry before, at and after actual maintenance, with changed deadline
   invalidating a previously shown intent.
5. An actual filtered recipient snapshot missing the receiver, with explicit
   scope and recorded-destination travel rather than claimed global loss.
6. Two successful inspections competing for the same final producer space;
   only the first actual delivery fits, the second keeps its obligation/value.
7. A finite Supply journey: accept, gather, fail at a full shop, save to disk,
   load with catalog validation, make space through an ordinary paid purchase,
   deliver and reject replay. No stock is erased or refilled for recovery.
8. Wallet-capacity recovery through actual spending, with total money conserved.
9. Dormant-owner guidance across all tracks and ready/missing-goods/expired/
   unclaimed contexts offers no action until the owner resumes.

`tests/guidance_capacity.rs` contains seven focused tests for complete
multi-output exchange, largest fitting batch, a real finished-item seller,
consumed inputs, a smaller batch with scarce ingredients, full valid catalog
inventory, near-full gathered inputs and an explicitly unvalidated SDK case.
Existing guidance, commitments, settlement, network, save and command suites
remain required regression coverage.

## Verification status

The integrator executed the five focused `terra-sandbox` suites with the pinned
toolchain using `cargo test --locked --offline`: `supply_readiness`,
`guidance_capacity`, `guidance`, `guidance_commitments` and `settlements`.
**All 70 tests passed**, including the nine new supply-readiness tests and seven
new inventory-capacity guidance tests. The retained [focused receipt](../v0.23-preflight/sandbox-integration.json)
and [complete log](../v0.23-preflight/sandbox-integration.log) record exit code
zero and bind the actual log SHA-256. Its recorded hash is
`0ee9785441e845229f90aa6fa124c4c49cab14728ccb70376228794b984a33b0`.

This is focused candidate evidence. Other implementation work continued during
that run; it is not the final frozen-source acceptance. The current release
[validation](../../VALIDATION.md) and canonical receipts identify the complete
executed gate roster and accepted source after integration.

## Peer correction

The reliability reviewer found that broad guidance still advertised actions for
dormant owners, even though the shared readiness and authority rejected them.
The top-level guide now returns an explicit resume instruction with no action,
before choosing any track. The new dormant-owner fixture covers all six tracks
and the four supply contexts without changing state.

## Rejected hypothesis and remaining limits

An initial low-energy handoff hypothesis was disproved: opening Command from
urgent guidance already selects the current owned active person, so an old
unrelated selection does not redirect Rest. It is not reported as a defect.

No populated-community policy, generic supply withdrawal/refund, migration,
new clock, command wire shape or saved schema is introduced. Physical Windows
and controller qualification, uncoached journeys, economic balance over long
careers and deployment-scale networking remain separate product work.
