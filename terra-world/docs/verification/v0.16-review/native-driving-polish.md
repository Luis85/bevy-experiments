# Native driving polish — candidate findings, corrections and final confirmation

Status: the three corrections below are implemented, independently source-reviewed
and included in the passed final release gates. The completed final driver
confirmation was also independently reviewed from seven native captures. Candidate
captures retain their original defect evidence; final captures below document the
corrected executable and the limits of this visual review.

## Evidence inspected

The native candidate used the original character/cabin example pack in a restored
Compact rental fixture. Its process receipt records executable SHA-256
`90448c7b042d2d87f811dfbe442005543e696c67f898ff8a9d64db97c1be94db`,
exit code 0 and unchanged source inputs during the capture run on 2026-10-29.
These are expert review observations from the native application, not an
uncoached player study or a claim about earning/travelling to the supplied fixture.

| Candidate capture | Visible finding |
|---|---|
| [Driving forward](../../screenshots/v16-candidate-driver-forward.png) | The footer repeats a hardcoded shortcut list above the configured driving help. The HUD shows a 177 m destination distance beside `Turn back · 423 m`, without explaining that the latter is the remaining route length. |
| [Driving panel before Exit](../../screenshots/v16-candidate-driving-panel.png) | The active/displayed vehicle is Comet hatchback `#1000073`, at 0 m, and Exit is available. |
| [On foot after Exit](../../screenshots/v16-candidate-human-exit.png) | The avatar stands beside the Comet, but the panel now displays Trail utility pickup `#1000053`, 201 m away, with Enter disabled. |

## Corrections and rationale

### One configured source of movement help

[`process_control_intents`](../../../crates/terra-demo/src/control.rs) now reports
the controlled actor's name and mode label. It no longer appends a second
hardcoded keyboard tutorial for either driving or walking. The existing
[`InputSettings::movement_hint`](../../../crates/terra-demo/src/player_input.rs)
continues to produce the live help from the user's configured bindings.

The initial review suspicion that P lacked a handler was incorrect. P is the
default `Binding::Park` and `driving_keys` submits the parking-brake action;
Space is the separate held handbrake. The actual issue was duplicated,
remap-insensitive help. No binding, input mapping or vehicle brake behavior was
changed by this correction.

### Label the distance that guidance actually calculates

[`guidance`](../../../crates/terra-demo/src/driving.rs) now renders
`Route guidance · Turn back · 423 m remaining` for the example value.
`NavigationPath.length_m` still sums the admitted route segments. It is not a
distance until the immediate turn. The destination distance, route planning,
refresh interval, bearing calculation and blocked-route reasons are unchanged.

The existing geographic-bearing regression now also checks the turn-back case
and the explicit `m remaining` suffix.

### Keep the displayed vehicle selected when leaving it

The panel resolves vehicle selection in this order: explicit owned selection,
active owned vehicle, then first owned vehicle. A normal Exit changes the active
body to the avatar. With no explicit selection, this previously made the panel
jump to the first vehicle, even while the player stood beside the car they had
just left.

`act(DrivingAction::Exit)` now pins the already-resolved displayed vehicle ID
before queuing the existing `ExitVehicle` command. This retains an implicit
selection of the active car and also preserves a deliberate selection of a
different owned vehicle. It does not redirect the Exit command: the sandbox
still resolves and validates the active vehicle, speed, ownership and supported
dismount placement. The adapter does not move the player or manufacture an exit
coordinate. A rejected Exit cannot switch authoritative bodies.

Existing queued paid-action context checks remain in place. The change neither
refreshes nor consumes their reviewed context and does not weaken selection,
restore, appearance, ownership, distance or affordability validation.

A separate reviewer traced the displayed-versus-active distinction, failed Exit,
explicit-other selection and the pending camera handoff. The review found no
remaining source defect: Exit remains an untargeted authority command, and the
camera transition still waits for the account to confirm possession of the
avatar. The reviewer also checked the configured help path, remaining-distance
meaning and the test's explicit fixture boundary.

## Focused regression and final confirmation

`exiting_keeps_displayed_vehicle_and_nearby_reentry_without_overriding_explicit_choice`
uses the existing fixture's ordinary paid Compact rental and Enter commands.
Explicit test setup then places the active car 500 m from the first owned vehicle
through the trusted snapshot-position helper. That setup isolates the selection
defect; it does not establish player motion, fuel consumption, earned distance or
economic profit.

The test runs both implicit selection and an explicit selection of the other
vehicle. It submits Exit through the adapter and the real sandbox command,
checks that the displayed choice is retained after authority changes to the
avatar, and checks that the other vehicle and the player's money are unchanged.
For implicit selection, Enter remains enabled beside the car and the real
re-entry command possesses that same vehicle. For an explicitly selected distant
vehicle, the selection remains intact and Enter remains unavailable until the
player is within reach.

The release owner's [final gate record](../v0.16-final/gates.json) reports all
43 gates passed, with 904 distinct passing Rust tests across 990 passing test
executions, zero failures and zero ignored tests. All 366 implementation inputs
remained unchanged. The demo gate passed all 160 tests; its
[retained log](../v0.16-final/demo.log) includes the Exit/re-entry regression,
the geographic-bearing/remaining-distance regression and the existing queued
paid-action context regression. Repeated test executions across gates are not
counted as additional distinct tests.

The [final driver process](../v0.16-native/v16-confirm-driver-process.json) used
the exact accepted executable SHA-256
`6ba842ceaa3a9efe88603ae81db48053606185655827b3b5cf657dbe5c296434`.
It ran from 05:26:46 to 05:32:37 UTC on 2026-10-29, after the final gates,
and exited with code 0 without an expected interruption. Its 366 source inputs
remained unchanged, and its starting and finished canonical source-map digests
match. The independent reviewer inspected the seven captures below with
`view_image`; this report did not start another game process or run Cargo.

| Final capture | Observed result |
|---|---|
| [Clear cabin view](../../screenshots/v16-confirm-driver-cabin-clear.png) | The driver is visibly seated within the cabin, with the head below the roof and arms within the glazing. There is no visible body protrusion or duplicate standing avatar. The longer `358 m remaining` guidance wraps inside the HUD without overlap. |
| [Clear first-person view](../../screenshots/v16-confirm-driver-first-clear.png) | The rider and exterior shell do not obstruct the view. The intended cockpit/dashboard remains visible. |
| [Driving panel](../../screenshots/v16-confirm-driver-panel.png) | The panel displays Comet hatchback `#1000073` while driving, establishing the selected vehicle before Exit. |
| [Selection after Exit](../../screenshots/v16-confirm-driver-exit-selection.png) | The standing avatar is beside the car. The panel retains Comet `#1000073`, reports `2 m away`, and enables Enter vehicle. It does not jump to the distant first-owned vehicle. |
| [Re-entered vehicle](../../screenshots/v16-confirm-driver-reentered.png) | The panel still displays Comet `#1000073`, and the footer again shows driving controls. |
| [Remaining route length](../../screenshots/v16-confirm-driver-route-remaining.png) | The HUD distinguishes `Town interchange · 19 m` from `Route guidance · Bear right · 27 m remaining`. The footer has one configured movement-help line, including `Space handbrake`, without the former duplicate shortcut list. |
| [Loaded paused fixture](../../screenshots/v16-confirm-driver-loaded.png) | The restored fixture shows the same remaining-distance wording and the explicit paused-clock hint alongside the single driving-help line. |

The complete cross-flow observations and native acceptance are recorded
separately in the [final native review](final-native-review.md) and
[native verification record](../../native-v16-verification.json). The candidate
captures above retain their original executable identity and are not substituted
for final acceptance evidence.

## Visual scope and deferred polish

No blocking visual defect was found in these final driver views. They cover one
saved character appearance at 1440 × 900 in the prepared local fixture. Static
images do not establish continuous animation quality, all body styles, loading
or failure transitions, concealed foot clearance or precise hand contact.
Those aspects retain their separate bounded geometry and runtime-test evidence;
this review adds no hardware/device qualification, uncoached usability result,
physical boarding animation or economic progression claim.

One nonblocking copy issue remains deferred: the re-entry status reads
`Comet hatchback rental · Worker`, while the following help line correctly says
`DRIVING`. A future status-label pass can make that role label clearer. The
selected vehicle, available controls and re-entry result are coherent in the
reviewed captures.
