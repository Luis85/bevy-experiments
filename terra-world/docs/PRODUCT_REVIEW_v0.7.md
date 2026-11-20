# Product review and dedicated improvement pass — 0.7

Review date: **2026-10-27**. The previous review is preserved in
[PRODUCT_REVIEW_v0.6.md](PRODUCT_REVIEW_v0.6.md). This increment addresses the
structural and gameplay gaps exposed by the transport/game-flow release: hired
fleets, contact between bodies, usable road routing, dormant player ownership,
input configuration, driving legibility and the failure paths between them.

## Product conclusion

Terra now connects company money, named employees, owned vehicles, custom routes,
finite passenger/freight demand, physical travel, wages and roadside service in
the same authoritative world as crafting, farming, shops, housing, ecology,
territory and politics. Offline players can retire their owned bodies and homes
from active memory without losing their identity, inventory or funded work.
Native controls, route visualization and animated wheels make the reference
transport loop more usable and readable.

This remains a bounded framework and reference game. It is not a completed AAA
MMORPG or a complete reproduction of Motor Town. The [capability
matrix](MOTORTOWN_PARITY.md) distinguishes implemented reference workflows from
partial experiences and absent features. No percentage-complete score or
concurrency claim follows from the number of systems or tests.

## Review across product perspectives

| Perspective | Improvements in this release | Remaining acceptance boundary |
|---|---|---|
| First-time player | Storage choice appears early in world setup. Source provenance remains explicit. Company, Drivers and Routes split the new workflow; the editable local passenger-loop suggestion provides a valid first route. | A richer tutorial, localized copy and long-term career pacing still need player studies. |
| Session trust | Unsaved controls follow menu/world handoffs; explicit Save keeps them after quitting. Capture cancels when leaving Controls; save/restore keys are suppressed during capture. Checkpoint status is concise. | Windows/macOS lifecycle and crash/power-loss usability need platform validation. |
| Input and accessibility | Fifteen keyboard bindings, conflict rejection, mouse/pad sensitivity, invert-Y, deadzone and gamepad world control. Dynamic movement hints, fixed Save Controls footers and visible controller status. | Menus and RTS selection still need mouse/keyboard. No physical device, screen-reader, localization, text-scale or color-vision certification. |
| Player agency | Register/fund a company, hire drivers, choose stops and loads, assign vehicles, dispatch, pause, resume and release. Mechanical failures have priced nearby roadside servicing. | No dismissal, player employment, rentals, lease return, company depots or general stranded-load cancellation. |
| Economy and progression | Fares come from finite treasury; goods and riders are reserved once. Wages enter separate employee wallets. Incoming demand and aggregate producer capacity are respected. | Finite reference content can run out of demand, funds or stock. Sustained economic balance and replenishment policy remain game-specific work. |
| Traffic and logistics | Continuous body admission, segment-projected access, right-side vehicle routes, directed patrol lanes, road-edge pedestrians, stationary-obstacle detours and paired service-bay reservations. | Full road law, intersection priorities, legal lane connectivity and arbitrary congestion recovery are absent. An impossible path remains blocked. |
| Driving presentation | Terrain-aligned wheel roll/steer, model-aware wheel/light rebuilding, cached advisory routes, turn-bearing cues and a north-up local map with roads/stops/traffic. | Geometric prototype assets, no suspension rig, detailed cabin animation, mirrors, vehicle audio or target-GPU performance claim. |
| Physical simulation | Continuous translation and conservative rotation sweeps, vertical deck separation, finite bounds and departure from existing overlaps. Shared local/host/autonomous stop/yield policy. | This is kinematic contact admission, without rigid-body impulses, deformation, tire contact patches, rollover, injury or articulated trailers. |
| Multiplayer and privacy | Protocol 5 private company ledgers, sequence replay checks, authenticated owner activation, explicit old-protocol/ALPN rejection and exact reconnect state. | No prediction/reconciliation, production accounts, moderation service, cross-server authority handoff or MMO concurrency evidence. |
| Persistence and scale | Disconnected owners, bodies and referenced homes can retire; frozen jobs, payroll, needs and loaded commitments survive restore. Immutable cold partitions reduce repeat work; a concurrent same-generation write is detected at the transaction barrier. | Complete bounded-world validation still pauses authority on one worker. Clone/install cost, global caps, one DB writer and dependency pinning remain. |
| SDK integration | Core contact primitives, domain company commands, geodetic routing and persistence lifecycle remain separate from the Bevy/native UI. Additive save defaults preserve catalog/source/schema identities. | Production games must choose physics, navigation, data ingestion, account services, assets, hosting budgets and migration policy. |
| Geographic and historical truth | Existing real-GPS, provenance, heightmap, source-layer and dated-pack contracts are retained. Reference population, roads and financial behavior are described as authored simulation. | A GPS coordinate does not establish real demographic, ecological or historic coverage. Live-provider availability and global ingestion were not certified by offline fixtures. |

## Findings closed during cross-review and polishing

| Finding and consequence | Change and evidence |
|---|---|
| Connected and disconnected owners were equally effective regional pins. | Persisted dormancy allows offline owned bodies and referenced homes to retire. Private cold-reference indexes preserve validation. Authenticated resume hydrates before activation. A 16-owner probe checks exact recovery. |
| Restart could treat saved capabilities as live connections. | Startup marks principals dormant; only verified connection lifecycle reactivates them. Duplicate Hello cannot trigger redundant hydration. Old protocol and rejected capabilities cannot cause activation work. |
| A competing SQLite connection could commit after snapshot composition without advancing the generation. | Recheck connection data-version inside the write transaction before mutation. A deterministic two-connection test preserves the previous generation and live state on rejection. |
| A dormant player-owned courier could retain a global delivery whose body was no longer resident. | A private cold cargo projection preserves dependency checks and freezes progress/retry/dispatch. Complete merged validation still checks the actual saved body; a forged receipt plus matching projection cannot bypass it. Pickup, loaded delivery and loaded return survive 2,000 dormant physical seconds. |
| Reconnect briefly published a default heading instead of the saved body heading. | The initial public actor uses the authoritative resumed body orientation; the cold-resume TCP fixture checks continuity. |
| Body contact added safety but exposed centerline and loading-area deadlocks. | Project access onto road segments; offset reference vehicle/pedestrian lanes; maintain directed patrols; admit bounded stationary-body detours. Reserve both loading bays together to avoid opposing dispatch. Populated physical-delivery tests remain required. |
| Native confirmation found a loaded pickup unable to finish a perpendicular turn beside stopped traffic. | A short forward clearance step may briefly increase waypoint distance only while separating from the same nearby stationary blocker. The one-metre/physical-speed bound, enclosing-circle guard, full terrain and body sweeps remain mandatory. A saved loaded-trip regression requires an actual delivery on the bundled heightmap. |
| Continuous movement could cross a thin body between endpoints. | Shared swept oriented prisms cover translation and conservative turns. Spatial entries relocate after each accepted move, so later bodies see the new occupied cells. |
| Mixed freight could exceed a producer's total capacity despite each item fitting separately. | Manual/company delivery and returns use aggregate stock admission. Rejected delivery preserves cargo and payment escrow. |
| Company travel could drain wages indefinitely behind an obstruction. | A physical-progress timeout parks the route, preserves its load and suspends payroll. Resume remains available for this blocked state. Waiting stock/queue/demand does not charge wages. |
| Assigned loaded vehicles could run out of fuel or condition away from a garage. | Nearby roadside service uses a quoted callout, existing service rates, company funds and finite garage fuel. It keeps the same trip, load and mileage proof; it neither teleports nor silently cancels a delivery. |
| The UI's selected vehicle/route could be mistaken for the driver's actual assignment. | Assignment feedback names the real bound vehicle and route. Driver status names the next stop and distance; freight selection exposes stock and destination targets. |
| Money wrapping moved the fleet navigation buttons as balances changed. | Company cash, personal money, fares and payroll use separate stable lines. Repeated blocked-route advice was shortened. |
| Leaving Controls could retain an invisible key capture. | Screen transitions cancel capture. The captured frame is consumed; F5/F9 and world actions remain suppressed during capture. |
| Launcher-only unsaved bindings disappeared when the game process started. | A bounded validated control handoff follows menu → game → menu. Explicit Save is still the only persistent preference write. |
| Save Controls was below a long scroll and initial focus opened the session editor at its bottom. | Save/Back live in a fixed footer outside the scrolling controls list. Keyboard focus no longer scrolls past the beginning just to reach Back. |
| Space reassigned to movement could also pause the RTS world. | Space yields to its configured action; pause/resume hints and the World clock button show only applicable shortcuts. RTS, walking and driving hints use the current bindings. |
| Focus loss could finish a pending RTS drag and select unintended actors. | Selection cancels its drag when the window is unfocused or input is suppressed. No order or selection mutation escapes that gate. |
| Native named/Shift Space could disappear in character/world names. | Complete the specific unhandled text-widget cases, preserving normal text, filters, selection and IME. Tests apply real queued text edits rather than inspecting labels. |
| Owned autonomous wheels received zero private actuator steering. | Manual controlled vehicles use actuator steering; autonomous and public replicas infer steering from accepted motion. Presentation never writes authority. |
| A restored/reused body ID could retain another model's axles and light dimensions. | Wheel attachment tracks model identity and replaces only wheel children; model-aware lamps rebuild their dimensions. Unrelated game-owned children remain intact. |
| Holding the calendar at 0× stopped wheel animation even though physical driving continued. | Only an actual simulation/virtual-time pause freezes wheel animation. Calendar rate is independent of the physical driving clock. |
| Guidance truncated the last domain route endpoint and could remain stale while parked. | Keep the complete bounded route including its goal; refresh after movement and periodically while stationary. Neutral “Route guidance” covers graph paths and fallback corners without claiming legal navigation. |

## Verification strategy

Review combines independent source ownership, domain invariants, actual Bevy
schedules, real TCP/TLS sessions, SQLite failure injection and native screenshots.
The first native batch followed creation, controls, fleet registration, funding,
recruitment, route authoring, dispatch, blocked payroll, safe release, driving,
minimum-size menus and saving. A combined fix batch follows the concrete findings;
final confirmation checks the changed paths. [VALIDATION.md](VALIDATION.md)
records the actual completed gates, counts and captures. Intermediate failing
runs are diagnostic evidence, never included in a “passing tests” total.

The regional probe separates submission, worker, polling and residency costs.
Retiring 15 of 16 synthetic owners demonstrates a lifecycle/residency property;
it does not mean 16 networked players were load-tested. The population probe
has no sockets or renderer. Native Mesa software rendering demonstrates layout
and interaction on Linux/X11, not a gaming frame-rate or hardware certification.

## Remaining product risks and next structural priorities

1. **Transaction scope and latency:** replace full merged-world validation with
   correctly certified affected dependency groups, preserving one writer and
   failure-atomic installation. Measure clone/install and main-loop stalls.
2. **Road and physical completeness:** explicit connected lanes and intersections,
   robust congestion/recovery policy, rigid-body integration, suspension,
   articulated trailers and source-derived collision/navigation validation.
3. **Sustained commercial play:** recurrent demand, replenishment, depots, rentals,
   employee lifecycle and safe loaded-work reassignment/cancellation. A paused
   loaded route still completes delivery before release; a permanently blocked
   path needs a clear world route. Offline commitments remain reserved.
4. **Online production foundation:** account enrollment/recovery, operational
   authorization, prediction/reconciliation, abuse handling and measured regional
   authority transfer. TLS and resume capabilities cover only part of this work.
5. **Presentation and access:** authored asset/animation/audio pipelines, hardware
   budgets, gamepad-only menus, wheel setup, text scaling and localization. Test
   dense streets, rain/night and cockpit comfort on declared target devices.
6. **Scale evidence:** representative connected bots, interest overlap, storage
   failures and long-duration churn on fixed hardware. Publish tick/frame tails,
   memory plateau, network bytes, authority pauses and recovery behavior before
   increasing any advertised population target.

These are bounded acceptance goals for subsequent increments. Existing aggregate
caps and the one-authority architecture remain explicit; they were not raised to
create an unsupported impression of MMORPG readiness.
