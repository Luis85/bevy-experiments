# Independent review of supply readiness and acquisition guidance

Reviewer: reliability/delivery specialist, 30 October 2026. Scope: WP03–WP04
implemented by the world/framework specialist. This is an independent source
review; Cargo execution was centrally coordinated and had not yet run when this
inspection was written. Final execution belongs to the current validation
record. No files owned by the world specialist were edited by this reviewer.

## Reviewed surfaces

- [Read-only supply preparation](../../../crates/terra-sandbox/src/settlement/supply.rs)
- [Authoritative settlement command commit](../../../crates/terra-sandbox/src/settlement/commands.rs)
- [Activities and acquisition guidance](../../../crates/terra-sandbox/src/guidance.rs)
- [Shared bounded inventory exchange](../../../crates/terra-sandbox/src/catalog.rs)
- [Supply readiness and continued-journey regressions](../../../crates/terra-sandbox/tests/supply_readiness.rs)
- [Acquisition capacity regressions](../../../crates/terra-sandbox/tests/guidance_capacity.rs)
- Existing settlement validation, expiry maintenance, dormancy and recipient-view
  contracts, inspected as the authority around the modified paths.

## Finding and correction

**One actionable consistency defect found and corrected by the owner.**
`activity_guidance` checked principal/account availability but did not reject a
dormant owner. The new delivery-readiness call covered only the later delivery
path: unclaimed offers could still advertise Accept, expired owned requests
Return, and missing-goods paths Craft or Gather even though command authority
and the new readiness API rejected that owner's actions.

The owner added one top-level dormancy guard before any track is selected. The
result has no executable action and explains resuming the owner while retaining
goods, funds and commitments. The added read-only regression spans all six
tracks with ready, missing-goods, expired and unclaimed supply contexts. This
closes the early-path disagreement without altering clocks, dormancy policy or
stored progress. The reviewer reread the correction and confirmed its placement
before acquisition, acceptance and return paths. A follow-up copy suggestion
distinguished inactive ownership from an ordinary local virtual-time pause.

## Authority and conservation assessment

No additional blocking value-conservation or preparation/commit divergence was
found in the reviewed code:

1. `prepare_supply_delivery` computes player debit, complete receiver credit and
   wallet credit before mutation. The actual command uses those prepared values
   after preparing again against its own current authoritative state. A previous
   successful inspection reserves no capacity or reward.
2. Player/account/ownership, expiry, catalog item, goods, exact receiving
   position, receiver inventory/producer capacity, wallet and three-dimensional
   reach are checked before the commit. The original authoritative command's
   outer dormancy/account checks remain. A failure preserves the entire state.
3. The funded request is removed only after its goods and reserved reward have
   moved to their proper owners. Return clears its claimant without spending
   carried goods or creating a refund. Progress counters retain existing caps.
4. Producer aggregate capacity and ordinary stack/item-kind limits are both
   checked. Two inspections cannot each reserve the last capacity; the second
   actual delivery prepares again and can reject. Tests retain escrow and the
   second accepted request after that refusal.
5. A receiving service absent from a recipient replica is described as outside
   the current view or unavailable, not as globally deleted. Travel retains the
   saved destination, and complete authority rechecks the receiving inventory.
6. Expired public offers are excluded until real settlement maintenance renews
   them. Exact request snapshots retain deadline, reward, quantity, destination
   and ownership across displayed intent; a renewal is not silently treated as
   the same reviewed request.
7. Craft suggestions reuse full `exchange` semantics: consumed inputs free
   slots before every output is credited. Bounded smaller batches are considered
   instead of assuming a desired quantity fits. Gather similarly reduces a
   bounded requested batch to available inventory headroom.
8. When crafting is blocked, a usable finished-item seller can remain an
   alternative. If no executable acquisition is available, guidance explains
   inventory space and opens inspection/selling rather than submitting an
   impossible gather or craft command.

## Test quality and limits

The new tests exercise command dispatch and complete state equality on
rejection, not just label predicates. They include exact funded payouts,
receiver/wallet failure and recovery, competing deliveries, expiry, spatial
replica absence, save-to-disk/load continuation, full byproduct stacks, consumed
inputs freeing slots, partial executable batches and near-full gathering.

The finite journey creates receiver space through an ordinary paid purchase,
then delivers after save/restore. Aggregate money and wood are checked before
and after; recovery does not erase inventory, inject stock or reset a treasury.
The adversarial 64-other-item inventory is explicitly marked invalid raw SDK
input, since a catalog-valid inventory cannot have more distinct catalog kinds
than the catalog admits. That honesty avoids treating an impossible saved-world
fixture as representative player behavior.

These fixtures are useful correctness evidence when executed. They do not
establish uncoached-player comprehension, sustainable long-career economics,
production marketplace behavior, network capacity or cross-client reservation.
The native consumer integration, current pixels and complete canonical run
remain separate release checks.

**Disposition:** source review accepted after the dormant-guidance correction;
execution and integrated-native acceptance remain with the release coordinator.

### Coordinated focused execution update

The release coordinator subsequently retained a successful focused run of
`supply_readiness`, `guidance_capacity`, `guidance`, `guidance_commitments` and
`settlements`: **70 tests across five suites**, exit code 0. The exact command,
log hash and candidate-only scope are recorded in
[the focused sandbox result](../v0.23-preflight/sandbox-integration.json).
Other source work continued during that run, so this result is not substituted
for the final canonical gate or native visual acceptance.

### Final coordinated acceptance

The later [actual terminal receipt](../v0.23-release-acceptance/completed-gates.json)
completed the full 44-gate roster against 381 unchanged implementation inputs:
1,070 distinct Rust tests and 1,171 Rust test executions, with no failed or ignored
tests and no timeouts. The separate
[accepted native provenance audit](../v0.23-native/confirmation-provenance-accepted.json)
rehashes that receipt and its evidence and binds the final direct-process
confirmation to the same accepted source and executable. The focused evidence
above remains preserved with its original candidate scope.
