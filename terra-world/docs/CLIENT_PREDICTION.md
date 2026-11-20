# Owner movement prediction and reconciliation

Introduced in Terra 0.8, the current client provides a bounded display preview for the locally controlled multiplayer
body. The server owns every position used by gameplay. Clients still send
velocity or pedal/steering intent; they never submit a position, cargo arrival,
fuel deduction, odometer value or economic completion.

## Authoritative acknowledgement

The current protocol sends a private `ServerMessage::MotionState` to each connected
principal alongside its public actor replication. Its `MotionSnapshot` contains
the principal ID, controlled body pose, completed server tick, latest input
sequence present at that fixed step, movement permission, current speed cap and,
for a registered vehicle, mechanical configuration/state and dimensions.

This packet was introduced with protocol 6. The current wire/TLS requirements
are maintained in [Compatibility](COMPATIBILITY.md); use matching client and
host executables when following this guide.

The server broadcasts immediately after its fixed step and before it ingests the
next input batch. Receipt of a command alone is therefore not presented as proof
that the command has been simulated. Vehicle information is sent only to its own
principal. Account inventories, other owners' vehicles and resume capabilities
do not enter this packet. Storage barriers continue to govern authoritative
state and command responses independently of the visual preview.

## Client history and rendering

`terra_net::prediction::ClientPrediction` is independent of Bevy. It retains a
bounded queue of locally sent input frames. Reconciliation validates the server
snapshot, ignores old ticks, rejects impossible or regressing acknowledgements,
removes acknowledged input, and replays remaining input from the authoritative
pose. Changing body or losing movement permission clears pending history.

The reference Bevy adapter sends movement every 50 ms, recording each discrete
input. Presentation can add a partial frame of up to 50 ms. Total predicted work
never exceeds 250 ms, and the queue additionally has a 32-frame cap. If recent
authority is absent or history exceeds that horizon, the preview returns to the
authoritative pose until a fresh snapshot arrives. Disconnection clears all
prediction state. Input sequence and time checks reject nonfinite or repeated
values before they change history. Socket receipt and preview share an updated
monotonic client clock, so a snapshot-receiving frame is not accidentally treated
as having occurred before its own reconciliation.

Vehicle integration calls the same `terra_core::drive_actor` used by authority.
Walking/flight previews respect the authoritative speed and locomotion type.
The Bevy adapter reads loaded source terrain, weather and buildings; checks a
bounded swept vehicle envelope; and clips against a transient copy of the shared
body-contact index. Translation plus corner arc travel determines quarter-metre
sample spacing, with at most 128 intervals per resolved input step; an oversized
envelope stops instead of being undersampled. Source underpasses compare the vehicle's
actual height and current support elevation with terrain-relative `min_height`,
using the same 0.15 m clearance margin as the local and headless authority.
Missing source coverage rejects movement. Resolver results
must preserve body identity/type, finite values, bounded speed and bounded 3D
displacement. A cumulative displacement bound is also checked against the
original authoritative pose: at most `max(speed_cap, 1 m/s)` times replayed
physical seconds plus one three-metre grounding allowance. That allowance is not granted again for every
queued frame. It cannot create an authoritative collision or injury.

Only `NetworkStatus::predicted_actor`, the owned actor mesh target and its camera
use the result. `SandboxState`, other actors, command origins, service reach,
money, fuel, cargo and physical travel proof retain received authoritative data.
The camera's smoothing and the remote mesh interpolation remain presentation
choices. RTS and AI bodies use their normal replicated poses.

## Integration

Add `TerraNetworkPlugin` after `TerraWorldPlugin` as before. The adapter installs
the prediction stage automatically. A custom client can use `ClientPrediction`
directly: record successfully sent input with its monotonic sequence and bounded
duration, reconcile private motion snapshots, then call `preview` with a local
collision resolver. The resolver must not mutate the game world.

F12 and optional telemetry expose reconciliation count, pending input count,
horizon resets and the distance between display pose and received authority.
That distance includes normal predictive lead; it is not an isolated network
error or accuracy metric. The actual socket-byte counters include private motion
messages; actor/sandbox delta-saving counters describe their own payloads.

## Acceptance and limits

Tests cover outstanding-input replay, duplicate/invalid sequences, stale ticks,
body transfer, pause, missing terrain, bounded history, shared vehicle dynamics,
malformed resolvers and source-footprint admission. A real TCP test verifies
that an input is not acknowledged as simulated before a fixed step and that a
peer never receives another principal's private motion state. The normal Bevy
client tests continue through the actual network schedule.

The 250 ms horizon is a deliberate reference policy, not a measured guarantee
under Internet latency. The client replays held intent over short local time
frames; the server retains its independent fixed-step schedule. This is not
deterministic whole-world rollback, lag-compensated combat, prediction of other
players, rigid-body prediction or a replacement for multiplayer load testing.
Source mismatches, authority corrections and congestion can still produce
visible correction. [Validation](VALIDATION.md) records what was actually run.
