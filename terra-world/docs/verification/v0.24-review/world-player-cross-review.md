# Independent world specialist review of native operations — 0.24

Reviewed on 30 October 2026 after the player specialist froze native source for
cross-review. This is an independent source and test-design review. The player
specialist owns the native execution receipts; the integrator owns real-window
candidate/final journeys and canonical acceptance. This report does not turn
source inspection or the previous release's screenshots into execution proof.

**Disposition:** no blocking source finding remained in the reviewed native
changes. The rental countdown correction identified by the integrator was
inspected after its implementation. No implementation edit was made by this
reviewer during this cross-review.

## Source reviewed

- `crates/terra-demo/src/reviewed_action.rs`, `industry.rs`,
  `fleet_operations.rs`, and `building_review.rs`.
- The exact selectors, operation eligibility, and command generation in
  `crates/terra-demo/src/fleet.rs`.
- Native `Activate` binding, labels, availability and layout in
  `crates/terra-demo/src/interface.rs`; queue dispatch and pending actor/world
  transition checks in `interaction.rs`.
- Review focus and its regressions in `crates/terra-demo/src/command_input.rs`.
- `crates/terra-demo/src/native_operation_tests.rs`, including actual rental
  progression and loaded handover tests.
- Existing authoritative damage behavior in `crates/terra-sandbox/src/commands.rs`
  and loaded handover in `transport/company/lifecycle.rs`, against which the new
  native presentation and retained operands were compared.

## Reviewed product boundaries

| Area | Source-reviewed behavior | Assessment |
|---|---|---|
| Explicit selections | Industry, employee, route, vehicle, stop, relief and depot selectors use the exact selected identity. A missing explicit selection returns unavailable. Initial selection may choose the first eligible entry only when no explicit identity exists. Deliberate browsing recovers a missing selection. | Removes silent substitution while keeping a usable initial state. |
| Industry operation | The displayed snapshot includes actor, restore revision, facility identity/name/position/owner, enabled state and recipe identity. Dispatch emits the captured desired enabled state. Production progress, stock and counters do not invalidate a still-current operation. | A displayed Pause cannot become an unintended Resume because the facility changed before dispatch. |
| Fleet operation | Thirteen ordinary operations bind displayed actor, tab, selections and action-specific operands. Assignment retains employee, vehicle and route. Cash and stock transfers retain exact amount/item/direction/destination. Route creation retains its name, ordered stops and service. | Current eligibility is checked again before command creation; a changed reviewed operand requires a fresh review. |
| Loaded handover | Both employees, source assignment, full trip, source vehicle/cargo, assigned route, and replacement vehicle/cargo are retained. Live reach, body speed, assignment and capacity remain eligibility checks. | The displayed replacement or relief cannot silently change. Authoritative handover still moves the exact trip and cargo while preserving the existing fare and travel accounting rules. |
| Building damage | The review retains the named structure and entire structure state, active actor and restore revision. Confirmation checks current permission, three-dimensional reach, exact health/structure state and visible Build context. | The native flow describes the existing authoritative 25-point operation and requires one-use confirmation; it does not add demolition refunds or change domain policy. |
| Focus and input | Fresh Build/Factions reviews enter on Cancel, wait for its usable layout, consume opening/closing acceptance input, and move focus off a disabled stale confirmation. | Review opening cannot replay the opening key onto a committing control. Cancellation and subsequent navigation remain reachable. |

### Rental countdown correction

The first Fleet design retained the entire `VehicleRental` in the operation
snapshot. Its normal `remaining_seconds` decrement would change that snapshot
between display and dispatch, rejecting otherwise valid operations during play.
The integrator identified this before acceptance. The reviewed implementation
now retains only the rental facility and reserved bond in `VehicleIdentity`.

This exclusion is narrow: vehicle ownership, model, cargo and trailer identity
remain retained. Assignment/current operation eligibility still runs at dispatch,
and Run explicitly rejects a rental with no remaining prepaid time. The snapshot
also omits fuel, odometer and payroll counters; trip identity, its escrow and its
handover accounting inputs remain retained.

The corresponding regression uses the real `RentVehicle` command and
`advance_transport(0.1)` between native activation and dispatch for Assign, Run
and Pause. It then advances the same rental to actual expiry, verifies that a
previously activated Run produces no domain command, and checks disabled Run
availability. This tests both useful tolerance of ordinary progression and the
meaningful expiry boundary. Its execution result belongs to the player's final
native receipt, not to this source-only review.

### Loaded handover and finite values

`DriverIdentity` includes the full `FleetTrip`, assigned vehicle identity and
assigned route. Both source and relief are captured. The replacement snapshot
includes its cargo, ownership, model, trailer and stable rental terms. The
operation token is consumed on command creation, preventing reuse of one queued
displayed operation.

The reviewed handover test obtains a real trip through the company simulation,
holds the loaded vehicle using the domain command, and uses a real purchased
replacement. Its fixture explicitly reallocates existing scenario money for the
purchase. Negative cases change relief/replacement/restore revision or execute
the real ReturnLoad command after activation. The positive case checks exact
trip continuation, relief/source assignments, unchanged company/player/treasury
balances and positions, moved cargo, and complete domain validation. It accounts
for the documented update of pickup odometer and transferred travel proof.

This confirms that the native intent binding is tested against actual domain
operations. It does not add a new handover or refund policy.

### Queue boundaries and presentation cadence

The real `Activate` observer converts ordinary Industry, Fleet and damage
controls into their retained reviewed variants. Raw `GoToSite`,
`ToggleProduction`, covered Fleet operations, `Damage` and `ConfirmDamage` reject
in the central dispatcher instead of resolving a fresh target there.

Reviewed variants reject while actor/body or world transitions are already
pending in the command/control queues, and while the network reports
disconnection. The shared native context also rejects missing/dormant players,
changed active-body identity and changed restore revision. Restore review clears
the remaining UI queue through the existing flow boundary.

Snapshots refresh with the displayed labels after persistence processing. Input
does not silently create a new snapshot. The new damage review clears its
displayed-confirmation binding until a subsequent label refresh presents that
review. These are local UI intent tokens; the serialized command protocol still
uses the existing authoritative checks and is unchanged.

### Damage consequences and safe recovery

The displayed structure is selected deterministically by full reach distance and
identity, named by blueprint and ID, and includes health before and after the
operation. Current permissions are checked both when beginning review and when
confirming. External health, owner, structure, actor, reach and restore changes
invalidate the prepared confirmation. Cancellation changes no world structure.

The destruction text matches the existing command behavior: destroying a
structure clears every player's home reference to it and returns no materials.
The confirmation token is consumed before issuing the command; replay cannot
produce a second damage command. The focus regression deliberately puts Confirm
before Cancel in child order, verifies entry on Cancel, invalidates health through
the domain damage command, and verifies safe focus recovery without replaying
accept input.

## Verification scope and remaining acceptance

The native regressions were inspected for real activation/queue/dispatch and
state-conservation assertions. This reviewer did not run a separate native
Cargo invocation or alter the player's execution history. Focused sandbox tests
and their exact source binding are separately recorded in
[world-framework.md](world-framework.md).

The integrator still needs the current full native test receipt, actual candidate
and final window journeys, canonical acceptance, and accepted binary/source
association. Physical controller behavior, Windows execution and other target
hardware remain separate qualification work; none is asserted by this review.
