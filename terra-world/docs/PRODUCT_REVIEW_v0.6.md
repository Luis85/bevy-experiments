# Product review and dedicated improvement pass — 0.6

Review date: **2026-10-27**. This increment extends the geographic world with a
driving/work progression layer, a complete native session flow, and the next
storage/transport foundations. The preceding review is preserved in
[PRODUCT_REVIEW_v0.5.md](PRODUCT_REVIEW_v0.5.md).

## Product conclusion

Terra is a reusable, bounded world-simulation framework with a playable reference
game. This increment connects character identity, possession, vehicle work,
finite resources, money, progression, regional persistence and remote authority.
It supplies the entry and exit flow needed to experience those systems without
constructing command lines.

The Motor Town comparison is a capability benchmark, not a declaration of full
parity. Nine original vehicle archetypes and several dispatch loops do not equal
the game's complete content, physical simulation, driving-device support or
commercial progression. The [capability matrix](MOTORTOWN_PARITY.md) labels
implemented, partial and open work and supplies acceptance gates. Production
assets, physics, accounts, distributed authority and realistic concurrency
evidence remain substantial work toward AAA presentation and MMORPG scale.

## Review across product perspectives

| Perspective | Delivered and reviewed | Remaining boundary |
|---|---|---|
| New-player journey | Menu → named character and appearance → source/world setup → review → loading → play; Continue, worlds, settings, controls and Join are connected. | No gamepad certification, localization or narrated onboarding. |
| Session trust | Unique destinations, source identities, saved profile/clock continuity, explicit restoration, final checkpoint and return handoff. | Windows/macOS native lifecycle and power-loss UX need platform validation. |
| Immediate play | Priced vehicle service, jobs, route markers, pickup/delivery, qualifications and time trials join existing gathering, farming, crafting, trade, housing, wildlife and territory work. | Reference settlement and authored activities, not a content-complete commercial game. |
| Driving | Shared pedal dynamics, physical-time fuel/mileage/wear, conservative footprint admission, first/third-person cameras, cockpit, lights and indicators. | No tire/suspension rigid-body solver, dynamic crashes, articulated trailers, winch, full police/fire simulation or wheel force feedback. |
| Economy/logistics | Finite wallets/station fuel, dealer transfers, escrow, reserved freight/passengers, delivery/cancel/expiry, trailers and driver progression share authoritative rules. | No companies, hired-driver payroll, custom commercial routes, insurance, rentals or balancing claim. |
| Needs | Physical-time driving fatigue and occupant-aware speed caps; avatar food, water and energy visible while driving. | Exit to eat, drink or rest; no deep occupant-health model. |
| Graphics | Original vehicle/profile meshes, bounded exterior lights, terrain-aligned bodies, bridge support, stop markers, navigation beacons and reviewed launcher lighting. | Prototype geometric assets; no AAA asset/animation/audio library or gaming-GPU benchmark. |
| Multiplayer | Protocol 4 sequenced driving intent; host motion, licenses, fuel and cargo; public appearance separated from private ledgers. | No prediction/reconciliation, public login/recovery, anti-cheat service or distributed sessions. |
| Security/operations | Verified TLS/ALPN, no fallback, bounded I/O, private capabilities and no-overwrite inspect/verify/backup/import/export. | Channel protection is not a complete account/deployment architecture; public fixture certificates are for tests. |
| Storage/scale | One bounded worker, nonblocking polls, guarded cold-page cache and durable acknowledgement boundaries. | Full dependency validation still pauses authority; main-thread clone/install, aggregate caps and disconnected-owner pins remain. |
| Reuse | Transport in terra-sandbox, dynamics in terra-core, separate Bevy/native/network adapters; stable source/catalog identity and save/schema versions. | Games supply production assets, physics, navigation, auth and source ingestion. |
| Accessibility | Focusable controls, text input/IME, scroll/focus handling, explicit prices/prerequisites, geographic coverage labels and contextual Escape. | Interaction audit, not screen-reader, controller, color-vision or localization certification. |

## Findings closed in the improvement pass

| Finding | Change and verification intent |
|---|---|
| Old throttle survived release/re-possession. | Successful control takeovers clear cached pedal intent while retaining replay sequence. Real TCP regression exercises the transition. |
| Generic movement/possession could bypass driving constraints. | Registered vehicles reject nonzero generic velocity; licensing/mechanical checks apply to possession and autonomous paths; manual adapters use driver-needs caps. |
| Atlas or absent manual input froze a possessed vehicle with saved nonzero velocity. | Unpaused physical steps brake the body when manual input is absent. Atlas sends protective stopped input. |
| Restored authority possession did not match the default RTS presentation. | Runtime-only restore revision triggers one deliberate control adoption, including successful JSON/regional restoration; ordinary revisions do not override RTS choices. |
| EnterVehicle already changed authority, bypassing camera initialization. | Presentation initializes yaw/pitch when entering the body even when authority has accepted it first. First-person/chase pitch no longer inherits the RTS angle. |
| Signals/park brake targeted an old showroom selection. | These controls resolve the actually possessed owned body; labels identify it. A stale-selection regression covers the command target. |
| Up-to-ten-litre refueling failed for players who could afford fewer. | Exact whole-litre offer is bounded by cash, tank space, finite station stock and ten litres; price is explicit and a zero offer is disabled. |
| Time trials had no route before starting. | Course-start marker is available before a run; course/job/garage targets are selectable, with active race gates taking priority. |
| Buttons omitted known prerequisites. | Enter/job/hitch/detach/race controls reflect licensing, reach, speed, assignment and capacity. Job labels explain common blockers; authority still checks changing shared stock and funds. |
| Vehicle needs readouts showed meaningless vehicle percentages. | HUD/status/needs bars show the avatar driver and explain park-and-exit for eating, drinking and rest. |
| Subnormal physical timestep could publish infinite acceleration. | Check every numeric VehicleStep output before assigning state; atomic-error regression covers the finite input edge case. |
| Custom vehicle_support could bypass can_enter or return invalid support. | Validate original/candidate positions and all requested/returned support points; custom-provider regressions cover rejection. |
| Missing terrain at a corner could charge collision wear. | Missing envelope coverage stops motion without impact damage. Known physical obstructions retain their collision response. |
| Dealer fuel, freight commitments and retry paths could threaten conservation. | Finite dealer/station transfer, NPC courier reservations, physical returns and single payout paths are covered by transport invariants. |
| Public replicas lost attached trailers or failed tagged profile deserialization. | Public attachment follows visible towing bodies; numeric profile keys use bounded pair-list delta headers; actual TCP replication exercises both. |
| Menu screen content ghosted and the avatar preview was too dark. | One window camera owns default UI; preview gets explicit lighting/exposure/framing. Native confirmation checks the result. |
| Escape opened pause before canceling an active edit or draft. | Text, placement and political drafts consume Escape first and suppress world input for that frame. |
| Managed worlds forced an overly close initial camera and technical chunk counts. | Wider landscape start; geographic coverage/readiness labels in setup/loading, detailed counts in diagnostics. |
| Save-and-return read resources from an empty post-run App. | Bounded external exit snapshot captured after final checkpoint; actual App::run regression proves the snapshot survives world consumption. |

### Native lifecycle and storage implications

The real save-and-return journey exposed an ownership error that helper-only
tests would miss: Bevy's `App::run` consumes the live app and leaves the original
handle empty. Final menu/settings data is now captured before the world drops
and read from an external channel afterward. On Unix, the supervisor replaces
itself with the next menu after its child and stderr close; other platforms use
a fresh-process fallback. This avoids an accumulating chain of supervisors.

Storage review retained one outstanding snapshot, revision/generation checks,
final drain before checkpoint, no dormant catch-up, source/catalog/schema pins,
no-overwrite maintenance destinations and durable success responses. Ordinary
Drive/Move/Focus/Ping traffic does not trigger a transaction for every packet.
This distinction is essential to continuous driving.

## Evidence and limits

The [validation record](VALIDATION.md) records final automated gates, actual
native interactions, the worker probe and release metadata. Screenshots are
captures of the running application. Linux/X11 with Mesa software Vulkan
establishes behavior on that configuration; it is not a gaming frame-rate or
cross-platform certification.

The worker probe separates submission, polling and transaction latency. Low
polling cost does not mean the transaction finishes within a frame. Rendering
during a transaction does not mean authority advances through it. Earlier v0.4
and v0.5 measurements remain archived and labeled.

The next structural priorities are affected-dependency-group transactions,
disconnected-owner dormancy, authenticated session recovery, measured regional
handoff, prediction/reconciliation and declared workload budgets. Driving needs
physical contacts/articulation, road routing, commercial company workflows and
input-device breadth. Presentation needs authored assets, animation/audio,
navigation legibility at speed, accessible remapping and hardware profiling.
Each requires its own acceptance evidence; increasing constants does not
complete that work.

The final independent screenshot review found no further material menu or
restored-control blocker. Two small UX follow-ups remain: make storage selection
visible earlier in world setup, and shorten repeated checkpoint/region diagnostics
in the pause screen. The native evidence records the full passenger pickup,
delivery and payment, two successful menu returns, preserved progression and
the absence of retained game processes after quitting.
