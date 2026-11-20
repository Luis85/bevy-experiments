# Settlements, public works and the economy

Terra includes authoritative gameplay territories and a funded work board in the living world. These are authored game rules. A neighborhood charter does not claim that a real municipality exists there, and source political borders remain a separate layer.

The rules live in `terra-sandbox/src/settlement/`. The native game and multiplayer host send the same `Command::Settlement` intents. The host supplies player identity and the active body's position; commands cannot submit balances, rewards or experience.

## A practical player loop

1. Gather wood, stone, berries and fiber, or trade existing goods at the market. Ore needs a tool, fishing needs a fishing rod, and advanced recipes need a workbench.
2. Accept supply work from Activities. The offer states its item, quantity, destination, credit reward and experience. Up to three contracts can be accepted at once.
3. Gather, buy or craft the requested goods. Move the active body to the destination and fulfill within 20 metres, measured in three dimensions. Goods enter the actual receiving production/shop stock.
4. Found a named neighborhood near the active body. Build homes, claim plots and plant fields within it. Contribute the next public-works project's materials and fund its treasury.
5. Upgrade to village and city when the project, experience and infrastructure requirements are satisfied. The territory and permitted plot count grow together.

The original avatar, possessed workers and possessed vehicles share one player wallet and inventory. Experience is also attached to the player. Needs belong to each body.

## Territory progression

| Tier reached | Radius | Regional plot capacity | Materials consumed | Credit requirement | Experience | Infrastructure inside the current territory |
|---|---:|---:|---|---:|---:|---|
| Neighborhood | 100 m | 6 | 20 wood, 10 stone | 50.00 credits deposited into its treasury | 0 | None |
| Village | 300 m | 24 | 30 planks, 10 stone blocks | 25.00 from the treasury | 50 | 2 owned/shared houses |
| City | 700 m | 64 | 120 planks, 50 stone blocks, 8 tools | 150.00 from the treasury | 250 | 6 owned/shared houses, 3 farms |

Founding transfers money from the player's wallet into the new treasury. It does not create another 50 credits. An upgrade pays its charter fee from that treasury to a market; project materials are consumed by public works. `Contribute` rejects unrelated goods or quantities above the remaining requirement. `Fund` transfers existing player money into the treasury.

The host checks prerequisites before any mutation. A failed contribution, funding operation or upgrade preserves inventory, money, territory and progression. `SettlementTier::upgrade_cost()` and `SandboxState::settlement_readiness()` supply the UI's requirements and current infrastructure counts.

There are at most 32 territories in the reference world and four founded by a single principal. These circular regions are tiered claims, not nested cadastral polygons. Existing small plots remain the footprint used by building and crop rules.

## Ownership and regional permissions

A territory is personal or faction-owned. Faction leaders and builders may operate its treasury and public works; ordinary members cannot mutate it. The original founder remains an audit identity and has no bypass after faction permissions change.

New claims cannot overlap another territory or cover another principal's existing plots. Expansion runs the same checks. `check_settlement_land` gates plot claims, construction and planting. `check_settlement_plot_capacity` provides the regional capacity limit.

New plots fully contained in a faction territory inherit that faction through `settlement_plot_faction`; their houses and crops then use the established faction roles. Plots cannot straddle a shared territory boundary. A transfer into an unrelated faction is rejected by `check_settlement_transfer`, preventing a private plot transfer from granting outsiders rights inside someone else's territory.

`check_settlement_interact` protects resource extraction: a personal charter permits its owner, while a shared charter permits current faction members. Existing private/work-plot checks remain stricter and cannot be bypassed by regional membership. Construction and planting still require leader/builder authority in shared territory.

## Persistent community membership

The faction lifecycle uses the same authoritative `SandboxState::apply` path as
construction and public works. Its commands validate membership and authority
before mutation:

- `LeaveFaction` lets a Member or Builder depart. `RemoveFactionMember` lets
  the current Leader remove another member, including an offline member.
- `TransferFactionLeadership` makes an existing member the sole Leader and
  retains the outgoing leader as a Builder. Role grants cannot create leaders.
  A leader must hand over before leaving an established faction. There is no
  inactivity election or member takeover.
- `DissolveFaction` lets an active sole Leader remove an unused faction only
  when it owns no plot and no settlement territory. All outstanding invitations
  are revoked and their affiliation slots are released. No property, public
  stock, treasury, private funds or obligations move; no refund is generated and
  IDs are not reused. A shared plot or territory blocks dissolution even when
  otherwise empty or unfunded.
- `DeclineFactionInvite` and leader-only `RevokeFactionInvite` release unused
  affiliation slots. Rejoining after departure needs a fresh invitation and
  grants the Member role, without restoring a former role or home.

Departure clears only the departing player's home reference when that home is
owned by this faction. The lookup includes cold regional housing descriptors,
so the host can revoke an offline member without loading or modifying the house.
The prior working set's reservation descriptors remain intact until regional
merge. Both the scoped and full-validation merge paths retain the unchanged
building and the cleared player reference. Saved bodies are not teleported;
personal property, personal homes, other memberships, inventories, wallets and
private obligations remain unchanged.

Faction assets never move back to their original creators when a role or
membership changes. Shared plots, houses, crops and territories continue to
refer to the same faction ID. Public stock and treasury balances are conserved
exactly through handover, removal, departure, checkpoint and resume. No lifecycle
action withdraws or refunds treasury funding or material contributions. The
original founder's audit identity grants no operational bypass after departure.
The new leader can continue public works after resuming; the outgoing Builder
retains those permissions until voluntarily leaving or being removed.

The native Factions panel shows the current leader, role and paged roster.
Leave, remove, leadership handover, dissolution and irreversible plot sharing require a named
review and explicit confirmation. The review retains player identity, restore
revision, selected faction and roster; plot sharing also retains the named plot
and affected property identities. A stale review cannot silently apply to a new
selection. Invitation acceptance and decline retain the exact invitation shown.
Native review is an interaction safeguard; the server independently enforces
current command authority. See [GAMEPLAY.md](GAMEPLAY.md#leaving-removing-members-and-handing-over-leadership)
for the player workflow and [COMPATIBILITY.md](COMPATIBILITY.md) for wire/save
compatibility. Elections, taxation, conquest, general treasury withdrawals and
transferring an existing territory between owners remain out of scope.

`SandboxState::faction_dissolution_readiness` is the read-only query shared with
command admission. On authority, all plots and territories are global even in a
validated regional working set. Shared houses and farms must belong to matching
faction plots, so the global no-assets check also protects valid cold property.
Unrelated cold pages do not prevent an unused faction from dissolving. A client
replica is a spatial subset and can expose visible blockers, but cannot establish
whole-world eligibility from local absence; the server always checks again.
The native review states this distinction and retains faction/roster/invitation
identity. A changed review cannot retarget another community.

Territories govern these land and extraction rights. They do not automatically transfer the ownership of an existing production business, alter source buildings, confiscate political areas, or create a sovereign country. Source-world and imported political geometry remain immutable inputs.

## Funded supply contracts

Contracts target actual production-input shortages first, followed by retail replenishment. The reference board posts at most four offers per minute of world time and holds at most 16 offers/accepted contracts in total. There is at most one outstanding request for the same destination and item.

Every offer's reward is removed from its issuing market's finite till when the offer is posted. The contract holds that money in escrow. Accepting claims the existing offer; it does not mint a reward. Fulfilling:

- verifies ownership of the accepted job, its deadline, reach and inventory;
- prepares the inventory and destination-stock changes;
- checks destination capacity and wallet overflow;
- transfers the goods, releases the existing escrow to the wallet, records experience, and removes the completed offer.

Retail work grants 25 experience and production supply work grants 40. Founding grants 10. Rank labels are Newcomer, Contributor, Steward and Pathfinder. Ranks are reference progression labels rather than combat attributes.

The journal can accept and cancel remotely. Fulfillment requires physical arrival. Cancellation releases the claim with its escrow intact. The configurable `SettlementWorld.contract_duration_seconds` defaults to three game days. Accepting starts a fresh deadline. Bounded maintenance releases expired claims before evaluating public demand: needed work renews, while confirmed obsolete work retires only after an exact, cap-safe issuer refund. A disconnected player cannot retain an accepted claim forever. Offline wall time is not automatically simulated; these deadlines follow persisted world time.

Snapshots include open offers and only the recipient's accepted work/progress. They do not expose another player's private journal. Partial replicas must not be advanced or saved as a complete authoritative world.

### Supply readiness and recovery

`SandboxState::supply_contract_readiness(player, contract_id, action, &catalog)`
inspects `SupplyContractAction::Accept`, `Deliver` or `Return`. It returns
`Result<(), SupplyContractBlocker>` without changing state. Acceptance and
return use the same checks as their commands. Delivery uses the same complete
inventory, receiving-stock and reward-wallet preparation that fulfillment
commits after every check passes. The blocker names an expired or unavailable
request, ownership/session state, the three-request limit, missing goods,
unavailable receiving data, receiver capacity, wallet capacity or 3D reach.

Readiness is an observation, not a reservation. The host prepares again when the
ordinary command executes. Two successful inspections do not reserve the same
last space in a producer. A recipient replica may omit the destination; that
absence means the current view cannot establish delivery readiness, not that
the whole-world service was deleted. Travel retains the request's recorded
destination and asks for another review after receiving its data.

Activities no longer advertises an expired unclaimed offer as acceptable while
waiting for bounded maintenance. An accepted request blocked by recipient or
wallet capacity keeps its exact goods, reward and deadline. The player can wait,
make room through ordinary production/trading/spending, or explicitly return
the request. Returning spends no carried goods and pays no reward. It releases
the claim while retaining escrow; subsequent public maintenance may retire
confirmed obsolete demand only after exact, cap-safe credit to the original
issuer. Accepted, unexpired promises remain honored. No general withdrawal or
treasury-refund mechanism is introduced.

Acquisition guidance prepares complete bounded inventory exchanges before
offering Craft or Gather. It considers all recipe outputs and consumed inputs,
can select a smaller executable batch, and prefers an available finished-item
seller when crafting cannot fit. A blocked acquisition explains the required
inventory space instead of repeatedly suggesting a rejected command. Recipes
inspect at most 20 batches and gather suggestions at most five quantities;
the existing resource shortlist and recipe-depth bounds remain.

## Market pricing and resident income

The existing shop inventory/till and integer minor-credit representation remain authoritative. Optional natural-resource prices follow the host catalog; old authored demo markets acquire supported new quotes without gaining free replacement stock.

Prices update once per game minute from a saved reference-price table. Scarcity raises the buy quote; abundant stock lowers it. The reference buy range is 80–150% of the anchor, subject to integer bounds. Sell quotes are bounded below the lowest buy band, including oversupply reductions, so buying and immediately reselling against a different stock quote cannot create profit. Anchors are persisted: repeated maintenance cannot compound a price multiplier indefinitely. The shop snapshot contains the same current prices used by transactions.

Personal Market actions use `LivingCommand::TradeQuoted` with the displayed
shop, item, quantity, buy/sell direction and exact total. A price change requires
review before any money or goods move; the server remains the price authority.
`SandboxState::personal_trade_quote` is read-only and shares execution's checks
for three-dimensional reach, quantity, stock, payer funds and both receiving
account/inventory bounds. The existing `Buy` and `Sell` commands retain their
current-price semantics for embedding games that intentionally do not request
a quote. Company depot trades keep their separate quoted company-wallet path.

Poor resident citizens can receive 1.50 credits at most once per world hour when the settlement has completed new production since the last payroll. Only residents with less than 10 credits are eligible. The issuing market pays from its finite till, and the transfer is capped by available funds. This keeps a small resident economy circulating; it is an aggregate participation-pay model, not a labor-contract or national-economy simulation.

No income is invented when a shop cannot afford wages, retail purchases or contract rewards. A depleted public economy can stall and needs player sales, supplied production and further circulation to recover.

## Company cash records and economic scope

Company cash movements persist as exact operating, capital and financing totals,
with the latest 64 monetary receipts retained for inspection. Stock purchases,
payroll, recruitment, fuel, repairs, callouts and public service contributions are
operating payments; depot access is a separate capital outlay. Owner deposits and
withdrawals are financing, not sales or profit. Accepted public fares and actual
shop-funded goods sales are recorded only when paid; loaded escrow is still held
value rather than company revenue. Household spending uses already-paid employee
wallets and does not count as a second company wage payment.

The owner-private `TransportCompany::accounting_report()` reconciles opening cash
plus the three classes of net cash flow to current company cash. Known personal
registration and payments on currently assigned vehicles appear separately, so
the same money is never debited twice. Older saves retain an explicit imported
opening balance; missing historical expenses and personally contributed stock or
vehicle costs are not assigned a guessed zero price. The result is a scoped cash
report, not an accrual profit statement or proof of indefinite economic balance.
The report and its exact retention, allocation, capacity and recovery rules are
documented in [COMPANIES.md](COMPANIES.md#cash-accounting-and-the-finances-report).

## Physical logistics and failure recovery

Dispatch ranks blocked production inputs ahead of covered inputs and retail demand, then compares stock coverage and travel distance. Source stock already reserved for another pickup is subtracted from availability. Several couriers cannot all promise the same four units of water.

A delivery progresses through `ToPickup`, `ToDropoff`, and, when necessary, `Returning`:

- Before pickup, goods remain at the producer and the job reserves them.
- At physical pickup, goods move from producer stock into the courier's cargo.
- At physical arrival, cargo moves into the destination stock.
- A stranded pickup can release its reservation without removing goods.
- A persistently blocked loaded courier or full destination triggers a physical return to the source. Cargo remains on the body until it arrives. If the return path is also blocked or source storage is full, it waits with its cargo intact.

Each job exposes bounded stall time, route retries and its last observed position. Up to three local route replans precede cancellation/return decisions. Replanning uses short alternate access points, but the host's terrain and collision resolver still accepts or rejects every step. There is no teleport-on-stall and no silent cargo deletion.

Road search uses bounded spatial node deduplication, adjacency lists and a deterministic priority queue. Supported source direction, access, numeric speed and grade metadata constrain the directed graph; an unavailable or over-budget restricted route cannot become a direct shortcut. Physical-time intersection permits coordinate automatic traffic with exit checks and bounded FIFO tickets. Conservative local detours and this admission scheduler do not provide a full navigation mesh, signal system, lane-changing traffic model or distributed pathfinding service. See [CONTACTS_AND_ROUTING.md](CONTACTS_AND_ROUTING.md).

## Integration hooks

```rust
// Authoritative lifecycle, after living production advances:
state.advance_settlements(sim_dt, &catalog)?;

// The native client and network host use the same transactional command path:
state.apply(player_id, Command::Settlement {
    action: SettlementCommand::AcceptContract { contract_id },
}, &catalog)?;

// Public bounded replica; retain existing SandboxView wire-size enforcement:
let view = state.settlements.snapshot(player_id, active_position, radius_m);
view.validate_for(player_id)?;
```

`SettlementWorld::validate(&SandboxState, Option<&Catalog>)` checks complete ownership, destination, escrow, price-anchor and sequence invariants. `SettlementView::validate_for` checks a deliberately incomplete, recipient-specific representation. The complete state has serde defaults at the sandbox extension field, preserving older worlds that have no settlement extension. World/source/catalog identity checks still apply when loading a host save.

The current territory tiers and recipe balances are reference policy in the sandbox module. An embedding game can replace this module/policy, supply its own catalog and scenario population, or keep the geographic/streaming layer alone. This release does not claim production MMORPG scale, account security, sharding, taxation, scheduled-offline progression or comprehensive economic equilibrium.

## Verification

`crates/terra-sandbox/tests/settlements.rs` exercises founding, faction authority, contributions, treasury conservation, real construction/farming prerequisites, expansion, supply fulfillment/replay rejection, expiry/reopening, privacy, scarcity anchors, finite pay, natural-resource markets, consumables and save corruption checks.

`crates/terra-sandbox/tests/supply_readiness.rs` checks shared read-only admission,
unchanged authority on rejection, expiry, partial recipient views, two deliveries
competing for final producer capacity, real paid capacity recovery and disk
save/restore continuation. `guidance_capacity.rs` checks multi-output/stack
capacity, input consumption, smaller feasible batches, full valid catalog
inventories and explicit adversarial unvalidated SDK input. Its raw-input case
does not claim a valid catalog can contain a 65th item kind. Executed acceptance
and platform limits remain recorded in [Validation](VALIDATION.md).

`crates/terra-sandbox/tests/faction_lifecycle.rs` checks atomic authority failures,
singular leadership, invitation-capacity release, revoked founder permissions,
personal-home preservation, cold/offline home removal through both regional
merge paths, conservation of public and private assets, and disk save/resume.
The native `communities` tests cover explicit review, cancellation, stale
selection/identity/restore/roster rejection, named invitations, property review
during normal crop growth, paged rosters and role-aware availability.

`crates/terra-sandbox/tests/faction_dissolution.rs` covers unused-faction removal,
affiliation capacity release, invitation revocation, monotonic IDs, rejection
without mutation, public assets/funds, unrelated cold/offline homes, both regional
merge paths and save/resume. Native community tests cover explicit cancel and
confirmation, expired context and remote partial ownership. The TLS lifecycle
target exercises real authenticated dissolution, revoked invitations and retained
command sequences through a host restart. Current executed results belong in
[Validation](VALIDATION.md).

`crates/terra-sandbox/tests/logistics.rs` exercises source reservations, stranded pickup cancellation, physical cargo return, deterministic connected-road paths, and validation/defaults for job-health metadata. Existing living-world and network tests continue to cover trading, needs, ownership, actual TCP state and authoritative movement.

## 0.24 useful funded work and public retirement

New requests use the actual remaining producer/retail need and a fitting amount
from 1 through 12. Receiver inventory limits, including the aggregate production
stock bound, are checked before reserving funds.

An unexpired accepted contract keeps its amount, reward, owner and deadline.
Demand changes alone do not revoke its delivery promise. Expiry or explicit
return releases ownership before public maintenance evaluates the offer.

Public offers retire only when a known matching receiver no longer requests
their item, is disabled, or has satisfied the target stock. Partial positive
demand and temporary full capacity retain the existing terms. Missing/replaced
receiving data is unknown and retains escrow. Still-needed expired work renews
under the same ID.

Retirement credits the exact escrow to the original issuer using checked money
arithmetic before removing the contract. A missing or capped issuer retains the
offer and its reward until recovery is possible. No funds are replenished,
redirected, split or silently discarded. Global IDs are not reused. Processing
is deterministic and bounded by the existing 16-contract board.

The public readiness API adds `SupplyContractBlocker::ObsoleteOffer`; guidance
and actual acceptance apply the same known-obsolete rule. This is a current
observation, not a reservation. Accepted delivery does not use that public-only
blocker. Save and protocol shapes remain unchanged.

