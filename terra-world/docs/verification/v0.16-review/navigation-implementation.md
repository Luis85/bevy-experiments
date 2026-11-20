# Navigation implementation and cross-review — 0.16

Date: 2026-10-29 (Europe/Berlin). Scope: G05, bounded autonomous movement in the
regional SDK. Final native and frozen release acceptance belong to the root
release record. These fixtures are authored SDK geometry, not observed human
play, licensed terrain coverage, dense-road throughput or hardware benchmarks.

## Observed failure and implemented correction

The accepted 0.15 driver could not pass one parked reference vehicle on a
9.5-metre supported strip. A 60-metre goal, with the obstruction 16 metres ahead,
remained 52.70 metres away after 120 physical seconds. It had travelled 286.42
metres through repeated forward/reverse clearances. The preserved
[failing baseline](navigation-baseline-015.json) identifies the accepted source
archive and navigation source hashes; it is explicitly not release acceptance.

The existing wide one-body and cluster proposals remain preferred. A final
parallel-parked-vehicle alternative uses projected body widths, a 0.8-metre
lateral margin and the existing enclosing radii plus 3.5 metres of longitudinal
room. Its two corners and rejoining leg require complete supported envelopes,
traffic clipping and body sweeps before the route changes. Moving, routed,
crosswise, self and unknown bodies cannot seed this narrower proposal.

A late approach may need more room than the original short clearance provides.
The planner can first propose a straight reverse of two to eighteen metres,
leaving nine metres before the first narrow corner. It admits the entire
retreat, both corners and reconnection before saving that three-point maneuver.
Every resulting movement uses ordinary bounded handling, braking, low-speed
direction-change arrival and live source/contact/traffic admission. A rejected
proposal never assigns a pose or moves the obstruction.

The additional work still shares **twelve corridor admissions per vehicle
update**, including every physical substep and clearance retry. Narrow/cluster
corners stay within 64 metres, complete proposals within 160 metres. The
unchanged 129-point route cap includes all unconsumed waypoints. A newly admitted
Move maneuver removes only its consumed prefix before insertion, so old retreats
cannot permanently exhaust recovery capacity. Rejected maneuvers preserve the
path. Patrols retain the original circuit/cursor and one front-consumed maneuver.

## Reconnection review and compatibility

The original incremental one-body maneuver now checks its complete reconnecting
**source envelope**. That future segment is limited to 160 metres of
three-dimensional travel before source callbacks run; a count of calls
alone would not bound a far waypoint's requested geometry. The final gate
review found that a 128–160 metre segment exceeded the compatibility provider's
per-call 512-interval allowance even when all terrain was present. The
correction splits the complete original pose sweep into calls bounded by
64 metres of translation plus rotation (0.1 metre tolerance), charging each
against the same twelve-call allowance. It preserves the final target and
commits no route until every piece succeeds. See the distinct
[navigation career regression report](navigation-career-regression.md) for
the failure, focused correction and acceptance status. An instrumented
two-kilometre waypoint test covers twenty maximum five-second updates and asserts
that no reconnection callback sees an overlong segment.

For the original maneuver, future body occupancy and intersection permission
remain decisions of the actual physical step. Requiring future occupancy and
permission to be clear at planning time broke both historical loaded-courier
escape cases; that attempted change was rejected. The final implementation
retains their **34.0 / 49.5 second** physical return results, exact goods/money
and exact save/resume. A future moving body can still stop the connecting step;
this source check is not a route reservation or collision exemption. New narrow
and existing cluster proposals use full traffic/body/source admission for every
proposed leg.

No save schema, wire contract, ownership, reward, inventory, fuel-accounting or
physical movement authority changed. The smaller corridor remains an optional
navigation suggestion. Actual admitted distance still drives existing vehicle
accounting.

## Focused verification

The [focused log](navigation-focused.log) and
[parsed record](navigation-focused.json) contain **63 passed tests**, zero failed
or ignored. Targets cover 18 library tests, 16 autonomous-driving tests, one
novice passenger-career test, five cluster tests, eight geometry tests, twelve
traffic tests and three historical recovery tests. That initial selection omitted the two long warehouse careers, and the
second integrated attempt exposed their reconnection regression. The distinct
[navigation career correction](navigation-career-regression.md) subsequently
passed **75 focused tests**, including the unchanged company-depot suite and
two stronger reconnection checks. The earlier log remains historical evidence;
full release acceptance is recorded separately by the release gates.

| Geometry case | Physical time | Accepted driver travel | Maximum source calls per update | Exact resumed updates |
|---|---:|---:|---:|---:|
| Parked body, 9.5 m strip | 17.4 s | 60.51 m | 2,538 | 172 |
| 5 m strip unavailable for 600 s, then 9.5 m restored | 619.7 s total; 19.7 s after restoration | 1,482.42 m total | 3,362 | 197 |
| Missing final six-metre connection, restored after 2 s | 21.8 s total | 101.28 m | 7,700 | 198 |
| Two physically moving crossings | 10.1 s | 59.74 m | 132 | 90 |
| Supported 9.5 m junction, blocked exit then explicit clearing order | 20.5 s total; 8.5 s after order | 59.76 m | 88 | 85 |

The long unavailable-flank total includes repeated safe clearances during the
600-second obstruction; it is not a zero-cost wait or economy-performance claim.
Calendar time is frozen. Each fixture independently checks all bodies' swept
motion in update order, physical displacement/heading limits, supported sampled
envelopes, unchanged parked bodies, conserved pocket money/inventories, complete
state validation and exact resumed continuation. The 32,768-source-call ceiling
is for these finite three-body 100 ms fixtures, not arbitrary provider runtime.

Three additional geometry checks cover a patrol's saved reverse approach,
protection of all 129 unconsumed waypoints, and the maximum-timestep distant-goal
bound. A new contact test rejects moving/routed/crossing/unknown/self narrow
obstructions. The existing three-parked-body cluster trace remains identical to
0.15, including 14.5-second arrival and 143 exact resumed steps.

## Remaining limits

This does not implement a global planner, moving-obstacle trajectory prediction,
future whole-route reservations, trailer articulation or a guarantee for every
narrow geometry. A missing or occupied rear approach, unsupported envelope,
exhausted remaining route capacity or an overlong rejoining segment can still
hold an order. The existing company/courier recovery and explicit new orders
remain applicable, with physical arrival and retained-value rules in force.

Implementation: [autonomy](../../../crates/terra-sandbox/src/living/autonomy.rs),
[contact proposals](../../../crates/terra-sandbox/src/living/contacts.rs),
[geometry matrix](../../../crates/terra-sandbox/tests/navigation_geometry.rs),
[Contacts and routing](../../CONTACTS_AND_ROUTING.md) and
[Vehicle dynamics](../../VEHICLE_DYNAMICS.md).
