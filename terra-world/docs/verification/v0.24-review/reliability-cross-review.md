# Independent reliability review of native operations and supply lifecycle — 0.24

Reviewed the final owned changes from the player and world specialists after
their implementation pass. This is a source and fixture review. Their retained
focused execution receipts and final integrated canonical run remain the
execution evidence; this review does not count those tests as another run.

## Native intent and operation review

Inspected `reviewed_action.rs`, `industry.rs`, `building_review.rs`,
`fleet_operations.rs`, the selector/availability/dispatch paths in `fleet.rs`,
`interface.rs`, `interaction.rs`, `command_input.rs`, `communities.rs`, and the
real activation/queue tests in `native_operation_tests.rs`.

- Context includes the player, restore revision and owned active body. Tokens
  are process-unique and nonwrapping. Replacing UI state cannot make an old token
  refer to newly reviewed intent.
- Explicit missing Industry/Fleet/faction IDs remain unavailable until
  deliberate browsing. A default selection can still be chosen where no
  explicit selection exists.
- Industry stores the named facility and displayed desired pause/resume state,
  including stable recipe/ownership/location identity. It does not snapshot the
  producer's changing stock or cycle progress. Ownership and reach are checked
  again before command creation.
- Fleet stores stable targets, selections, load/assignment, route, item,
  quantity and transfer direction. Current eligibility is rechecked. Root's
  earlier finding about a continuously decreasing rental timer is resolved:
  review retains facility/bond terms, while current Run availability rejects an
  expired, missing or no-longer-owned vehicle. Ordinary countdown progress does
  not invalidate Assign/Run/Pause intent.
- FleetTrip's retained odometer/time fields are pickup identity, not per-frame
  counters. Handover explicitly changes that identity; routine travel does not
  churn the review token. The new loaded-handover fixture changes relief,
  replacement vehicle, restore revision and returning-load state before dispatch,
  then checks the exact actual successful trip/cargo/escrow/body result.
- Building damage first names the structure and health consequences, then
  creates a separate one-use confirmation. Its full structure snapshot, current
  permission, actor and reach are checked again. Cancel changes no structure;
  the focus/scroll integration starts on Cancel and returns to the review
  origin. Destruction text explicitly includes clearing homes and no material
  refund.
- Reviewed dispatch rejects pending body/world transitions and disconnected
  transport state. Bare action variants do not bypass required reviews. Domain
  commands still own authoritative eligibility and mutation.

No additional blocking source defect was found after the rental correction.
The review does not certify visual reflow, real controller behavior or native
screenshots; those have separate integration evidence.

## Supply lifecycle, conservation and atomicity

Inspected `settlement/demand.rs`, `economy.rs`, `supply.rs`, `commands.rs`, model
and validation contracts, the guidance interaction, and all 13 new
`supply_turnover.rs` acceptance scenarios.

- Demand distinguishes known obsolete work from missing/replaced receiving
  data. A filtered replica with no receiving service therefore does not invent
  global obsolescence. Authority performs the current acceptance check.
- New posting derives useful one-item through twelve-item batches and applies
  inventory admission, including the producer's aggregate 2,000-unit bound.
  Capacity evaluation occurs for posting; per-frame readiness/guidance borrows
  the observed inventory and does not clone it merely to classify demand.
- Unexpired accepted promises are left unchanged by maintenance. Delivery still
  prepares exact player stock, receiving stock and wallet before mutation, so a
  changed recipe, disabled producer or later filled demand does not erase the
  promised reward. Existing reach, catalog and capacity admission still applies.
- Expiry releases ownership before public policy, and explicit return releases
  it immediately. Neither takes carried goods or grants progression. Unknown
  demand retains terms/escrow; needed expired offers renew under their same ID.
- Obsolete public escrow is credited only to `issuer_shop`. Checked addition
  and `MAX_MONEY` admission precede both the credit and contract removal. A
  missing or capped issuer leaves the exact value in place, without a partial
  refund or substituted recipient. Claim release on expiry is an explicit prior
  lifecycle transition, not a failed refund that silently changed money.
- Contract IDs are traversed in `BTreeMap` order within the existing 16-offer
  cap, giving deterministic competing refunds. Posting acquires the next valid
  ID before debiting funds; retired IDs are not recycled. New work does not
  reserve money when capacity or pricing admission fails.
- The fixtures check unchanged state after obsolete/competing acceptance
  rejection, actual issuer spending after disk restore to release a capped
  refund, exact original issuer credit, no second refund, and a saturated board
  recovered by real quoted trades followed by new accepted/delivered work after
  save/load. Direct fixture setup is labeled as setup rather than gameplay.

No blocking correctness or partial-mutation defect was found in the reviewed
scope. A capped issuer may retain a board slot until it spends money; this is the
documented finite-value policy. Choosing an alternative refund destination or
discarding the escrow would require a different explicit product contract.

The world specialist's retained six-suite run passed 83 tests; the player final
suite was queued after the reliability persistence run at review closeout.
Final native and release acceptance should be read from their actual completed
receipts, not inferred from this source review.
