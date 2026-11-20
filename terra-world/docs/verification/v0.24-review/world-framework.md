# World framework review and implementation — 0.24

## Review scope and evidence

Reviewed the delivered 0.23 product intent, complete G01–G18 register,
settlement/economy and architecture documentation, domain contracts, supply
readiness, guidance, community lifecycle, living production and inventory
validation, regional working-set boundaries, company recovery and existing
continuation tests. This is source and domain review; native screenshots and
physical-device/user qualification belong to their separately recorded owners.

| Priority | Existing evidence | Product consequence |
|---|---|---|
| P1 | `settlement/economy.rs` renewed every expired contract without evaluating public demand. | Obsolete public work could keep issuer money and all sixteen board slots indefinitely. |
| P1 | Posting did not apply receiver inventory admission or the producer's 2,000-unit delivery bound. | A newly advertised job could already be impossible to deliver. |
| P2 | Posting clamped shortage to a minimum of four. | A one-item shortage reserved funds and requested excess goods. |
| P2 | Public acceptance checked owner/time/count, without a current-demand check. | A player could accept an offer after its actual need disappeared, before maintenance. |

The selected scope is an economical continuation of G03/G04, rather than a new
quest ledger. Community absent-leader succession and populated dissolution need
their own authority/property/treasury policy. Dense navigation, representative
licensed geography, physical performance and distributed authority still need
their documented workloads and deployment requirements. Increasing hard caps or
inventing automatic takeovers would not supply that evidence.

## Primary research and applied reasoning

- [PostgreSQL transactions](https://www.postgresql.org/docs/current/tutorial-transactions.html)
  explain all-or-nothing updates. Applied here by preparing the exact issuer
  credit before changing either the issuer or the escrow holder. No database
  dependency or claim of a distributed transaction was added.
- [PlayFab items and inventory](https://learn.microsoft.com/en-us/gaming/playfab/economy-monetization/economy-v2/inventory/items-and-inventory-overview)
  describe atomic inventory batches and retry identity. Applied as conservation,
  revalidation and single-application invariants in the existing domain engine.
- [EVE Online's March 2020 patch notes](https://www.eveonline.com/news/view/patch-notes-for-march-2020-release)
  provide a primary game-economy example of fully funded orders and returning
  escrow when orders are removed. Terra's exact demand/retirement policy is an
  explicit product decision in the 0.24 plan, not a rule prescribed by EVE.

Sources were searched and read on 30 October 2026. No external service was added.

## Implemented policy and boundaries

`settlement/demand.rs` derives three outcomes from the observed receiving
service: unknown, obsolete, or still needed. A needed result borrows the current
inventory and retains the useful shortfall; only new posting evaluates the
largest current batch in 1–12 that fits, or skips it when capacity prevents any
posting. Guidance, readiness and lifecycle classification therefore do not clone
inventories for their per-frame read-only checks. All callers share the same
demand definition. Goods in all producer inventory stacks participate in its
2,000-unit delivery limit.

An accepted, unexpired request remains a promise with the original goods,
quantity, reward and deadline. Demand changes do not invalidate its delivery.
Expiry releases ownership before public maintenance; explicit Return releases
ownership immediately and keeps escrow until maintenance. Both preserve carried
goods and progression.

Known obsolete public offers return their exact escrow only to their recorded
issuer. A checked addition must fit within `MAX_MONEY` before removal and credit
commit. Missing issuers, replaced/missing receiver data and blocked refund
capacity retain value. Refund competition is processed in ascending contract-ID
order, bounded by the existing sixteen-request cap. Retired IDs are never reused.

A remaining positive shortage retains an existing public offer's original
quantity and reward, even when less than that quantity is now missing. Temporary
capacity exhaustion likewise does not revoke a funded offer. Still-needed
expired offers renew under the same ID. These distinctions preserve published
terms and avoid confusing lack of capacity with lack of demand.

The additive `SupplyContractBlocker::ObsoleteOffer` supplies the same explanation
to readiness and actual acceptance. Guidance excludes known obsolete public
offers. An absent destination in a filtered replica remains unknown; authority
checks the current receiving service when the command executes.

No serialized command, stored field, schema number, clock, wire version, money
source or external package changed in this work stream. A blocked issuer can
still hold board capacity until it spends money; deletion or an invented refund
destination would violate the finite-value contract.

## Focused acceptance

The new `supply_turnover` suite covers useful one-item production/retail requests,
aggregate capacity, read-only readiness and atomic rejection, preservation of
accepted terms, original issuer refund, no repeated refund, deterministic
competing refund order, three-player acceptance, expiry/return, unknown and
filtered receiving data, disk continuation and a saturated sixteen-slot board
recovered through real quoted trades.

The capped-refund continuation uses a real issuer purchase from the player to
create refund capacity after disk restore. The saturated-board continuation
fills demand through actual trades, creates new demand through an actual
purchase, accepts newly posted work, saves/loads, delivers and checks exact goods,
money and progression. Direct fixture edits establish initial conditions; they
are not claimed as gameplay actions.

### Initial execution and fixture corrections

`world-focused-01` compiled the requested six suites and ran guidance (23 passed)
and capacity guidance (7 passed), then stopped with three failures in the
commitments harness. Those older fixtures replaced a public request's item
without giving its receiver corresponding input demand. The new public-demand
policy correctly rejected that setup. The corrected fixtures explicitly author
the receiving demand and retain the original intent, seller, amount and cost
assertions.

The existing readiness fixtures likewise now give their authored requests real
input demand. The capacity competition fixture sets its initial inventory before
authoring the request. The full-retail-receiver continuation first accepts while
demand exists, then establishes the later full receiver as an initial condition
for the conservation portion. Both original guidance step assertions, capacity
rejection, actual spending, disk recovery, goods, money and once-only progression
assertions remain. No production check was weakened to accommodate a fixture.

The already-built new `supply_turnover` harness then passed all 13 tests directly;
its exact executable hash and command are in `world-turnover-direct-01.json`.
The initial build did not capture a prebuild source-hash map, so that direct
receipt proves binary execution and does not claim a cryptographic compile-to-
source binding. Only the two old fixture files had changed after that build.

The six-suite `world-focused-02` rerun includes the lazy capacity improvement and
corrected fixtures. It passed **83 tests**, with no failures or ignored tests,
in **78.726 seconds** including compilation:

| Harness | Passed |
|---|---:|
| guidance | 23 |
| guidance_capacity | 7 |
| guidance_commitments | 9 |
| settlements | 22 |
| supply_readiness | 9 |
| supply_turnover | 13 |

The invocation finished at `2026-10-30T11:34:28.011460+00:00`, exit 0. All 121
captured core/sandbox/workspace inputs were unchanged through that completion.
The integrator appended the current supply policy to the sandbox README at
`2026-10-30T11:34:38.732165+00:00`, after the run ended. The separate
`world-focused-02-postrun-documentation.json` records that exact documentation
delta; implementation inputs remain unchanged. Complete command/result/log/input
receipts are under `../v0.24-preflight/`. The final canonical run remains the
release-wide acceptance and binding for current source, native executable and
all evidence.
