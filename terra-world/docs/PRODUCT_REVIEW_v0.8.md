# Product review and dedicated improvement pass — 0.8

Review date: **2026-10-27**. The previous release review is preserved in
[PRODUCT_REVIEW_v0.7.md](PRODUCT_REVIEW_v0.7.md). This increment addresses the
remaining operating gaps exposed by commercial fleets, occupied roads, dormant
owners and the native game flow. It also adds bounded owner prediction, trusted
host access controls and a measurable reduction in repeated regional work.

## Product conclusion

Terra 0.8 connects finite company and household money, depot inventory, prepaid
rentals, physical recovery, directed roads, intersection admission and owner
lifecycle in the existing world. Its reusable command boundary remains the same
for the native demo and authoritative host. Menus support a controller and an
on-screen keyboard; configurable text, camera and calibrated wheel input widen
the usable input paths. Current implementation and validation are documented
rather than inferred from feature names.

This is a bounded SDK and playable reference game. The current source is not a
finished AAA MMORPG, a global historical reconstruction or full Motor Town
parity. The [Motor Town matrix](MOTORTOWN_PARITY.md) and
[MMORPG foundation](MMORPG_FOUNDATION.md) distinguish working reference behavior
from production content and infrastructure that still require development.
No completion percentage or concurrent-player claim follows from test counts.

## Review across product perspectives

| Perspective | Implemented improvement | Remaining acceptance boundary |
|---|---|---|
| First-time player | The complete character → source/world settings → review → play flow remains. Activities now points toward driving jobs, garages/rentals and fleets. Fleet tabs separate Company, Drivers, Routes, Recovery and Depot. | Player studies, a richer tutorial, longer career pacing and localization remain. |
| Player agency and recovery | Hold a loaded vehicle without payroll, physically return to the original pickup, or hand over to a nearby qualified relief employee and compatible empty vehicle. Dismissal settles wages. | A physically impossible route can remain blocked. These actions do not guarantee recovery from arbitrary terrain, congestion or every future vehicle class. |
| Economy and production | Paid employees and departed households can buy actual goods/transit. A player contribution moves company money into the public issuer. Depot transfers move actual inventory. | Demand is a finite reference abstraction. Long-duration balance, individual commuter schedules and manufactured rental fleets are not established. |
| Rentals and progression | Prepaid five-minute rental, finite garage fuel, refundable bond, renewal, expiry and quoted physical empty return form a complete lifecycle. Expiry preserves loaded obligations. | Mileage leases, insurance, detailed parts and a finite manufactured dealership economy remain game work. |
| Traffic and logistics | Source one-way/access/speed/grade metadata feeds a bounded directed graph. Physical-time intersection queues check exit space, persist tickets and requeue stalled permits. | Complete road law, surveyed lane connectivity, signal phases, arbitrary congestion recovery and global navigation are absent. Manual road-law guidance remains advisory. |
| Driving presentation | Heightfield-draped road arrows, stop areas, lane edges and center dashes expose available source semantics. Legal/blocked route feedback avoids drawing forbidden direct shortcuts. Owner display prediction reduces local response delay. | Prototype geometry, sparse repeated environment assets, no detailed suspension rig, mirrors, vehicle audio or gaming-GPU performance certification. |
| Physical admission | Vehicle-height-aware bridge/building clearance uses sampled terrain as its datum. Thin source obstacles are sampled at bounded quarter-metre translation-plus-corner-arc spacing; oversized sweeps reject. | These remain sampled/kinematic admission rules. Tire contact patches, rigid-body impulses, articulated trailers, deformation, injury and rollover remain open. |
| Input and accessibility | Main/pause menus support controller focus and text drafting. Text scale is 100–150%; first-person vertical FOV and driving auto-center are configurable. Bevy/Gilrs wheel/joystick profiles support calibrated axes and neutral rearming. | In-world management and RTS remain mouse/keyboard flows. Physical-device certification, force feedback, screen readers and localization remain unverified. |
| Session trust | Queued text cannot apply after focus loss; disabled fields cannot open a controller editor. Draft cancellation, preference handoff, stable save controls and quote confirmations preserve intent. | Windows/macOS lifecycle, platform clipboard and crash/power-loss UX need separate platform testing. |
| Multiplayer and privacy | Protocol 6 adds private completed-step movement acknowledgements. A 250 ms display preview reconciles outstanding input; game state remains authoritative. Trusted APIs suspend principals or rotate capabilities after durable commit. Resume-only admission rejects new anonymous allocation. | Public accounts/recovery/moderation, Internet latency/loss qualification, combat rewind, distributed handoff and production MMO concurrency remain open. |
| Persistence and scale | Immutable environmental dependency certificates and exact equal-page reuse reduce repeated work. Current ledgers and dependent cold bodies still validate. Stage metrics identify the work saved. Real TCP/TLS churn tests include offline owners, restart and a write-lock failure. | One authority/writer, bounded aggregate populations, cloning/pinning/global dependency work and authority pauses remain. Short synthetic probes do not establish capacity or long-term RSS plateaus. |
| SDK and data integration | Domain commands, core collision/clearance, road planning, prediction and persistence stay separate from native UI. Additive defaults preserve compatible version-one saves. GPS, source/catalog identity, provenance and dated packs remain explicit. | Games must select production physics, assets, ingestion/licenses, identity services and migration/hosting policy. Authored fixtures do not establish real ecological, demographic or historical coverage. |

## Findings closed during cross-review and polishing

| Finding and consequence | Correction and evidence contract |
|---|---|
| Cold partition certificates still left repeated full environmental validation and serialization on every checkpoint. | Dependency-closed unchanged environmental groups reuse immutable proof; equal pages reuse exact bytes. Current global ledgers and actual dependent cold records validate together. Complete untrusted restore retains full validation. The regional probe compares canonical output and stage timings. |
| Loaded fleet work had insufficient recovery choices. | Hold, physical return and near-vehicle relief handover preserve goods/passengers, payment escrow and mileage proof. Failed capacity, reach, funding, staffing or quote admission leaves the commitment intact. |
| Employee dismissal could lose earned money, while wages accumulated without recurring retail demand. | Settlement conserves fractional wages and moves departed funds into a durable household ledger. Bounded eligible spending buys real stock and transit with existing money. No offline catch-up or free replenishment is introduced. |
| Rental expiry or return could discard obligations or create money/fuel. | Expiry stops driving without deleting the vehicle/load. Return requires the original garage, a stopped exited empty vehicle, no assignment or attached trailer, and a current quote with finite refund/stock capacity. Rejection preserves the rental. |
| Safe intersection contacts could deadlock four approaching cars. | Exit preview follows the outgoing lane segment instead of shrinking its lateral offset toward the intersection center. Four-way tests require actual FIFO progress and contact bounds, including save/resume. |
| A route above the bounded detour budget could degrade into a direct shortcut through road restrictions. | Over-budget routes stop and report unavailable. Native guidance suppresses the route line and displays the reason. Directed-route and caption regressions cover the boundary. |
| Reverse one-way seeding used the wrong segment-distance origin on unequal segments. | Reverse population placement follows reversed traversal with separated starts and permitted heading. The focused traffic regression exercises the unequal-segment case. |
| A high building base was treated as unconditional vehicle clearance. | The actual model height and ground-relative underside determine clearance with a margin; nonfinite/unknown inputs fail conservatively. Core, host surface, swept authority and Bevy tests include low car versus tall bus. |
| One-metre source sampling could miss a thin wall between vehicle poses. | Manual, host and preview envelopes include quarter-metre translation and corner-arc sampling with explicit step caps. A host regression starts clear and crosses a wall between the former samples. |
| The prediction clock could appear older than a snapshot received during polling. | Use the actual receive timestamp for both reconciliation and the current display sample. The preview remains bounded by the freshness horizon. |
| A custom prediction resolver could accumulate a fresh altitude allowance on every tiny input. | Check identity, kind, finite pose/velocity, speed and 3D per-step distance, then enforce a cumulative authority-to-preview bound with one grounding allowance. Adversarial tiny-frame tests cover this escape. |
| Prediction diagnostics labeled ordinary unacknowledged lead as a correction. | The diagnostic is explicitly display-to-authority gap. It includes normal input lead and must not be interpreted as server correction error or a latency benchmark. |
| Focus loss could leave queued virtual-keyboard edits, and disabled fields could open an editor. | Input suppression clears pending actions, preserves the private draft, and gates disabled text/button state. Actual widget/system tests cover apply, discard, stale edits and disabled fields. |
| A configured wheel could leak released pedals into ordinary gamepad movement when wheel driving was disabled. | Exclude the configured device from ordinary pad sampling, even while disabled. Missing/nonfinite axes report useful feedback; focus loss/reconnect/profile changes require neutral rearming. |
| A bound wildcard address is not necessarily a usable capability resume destination. | The trusted rotation API accepts a validated explicit client-facing endpoint. Invalid endpoints fail before mutation; regional write failure preserves the prior token, policy and live peer. |
| Suspension could be mistaken for a person-level ban while anonymous registration stayed open. | Document exact-principal scope and add deployment-owned resume-only admission. Rejected registration allocates no player and performs no owner hydration. Public enrollment/recovery remains a separate service. |

## Additional whole-game review and polish

The follow-up review traced the older survival, gathering, crafting, farming,
housing, faction and territory workflows through the new transport and regional
systems. It also reconciled the complete gameplay/data/network documentation
with current commands and save behavior. Three concrete integration findings
required domain/test changes:

- **Construction versus vehicles:** the former vehicle-center radius allowed a
  new object to intersect the end of a long bus. Authoritative Build and native
  preview now share actual oriented model/body dimensions with vertical overlap.
  An actual rented-bus regression rejects construction atomically, permits clear
  side placement and also checks animal footprints. Movement overlap escape is
  retained.
- **Quick gathering at territory boundaries:** the shortcut filtered protected
  plots but could repeatedly choose a protected territory node ahead of an
  accessible nearby node. It now applies current territory rights before choosing
  the nearest target. A real founded-territory regression gathers the public
  resource and leaves the protected stock unchanged.
- **Verification isolation:** a real F5 input test used a default save path in
  the source tree. It now creates a private unique fixture, proves the checkpoint
  was written there and cleans up. Generated state is excluded from the archive.

The native review at 150% text also exposed clipped inspector navigation. The
inspector and Fleet tabs now switch to two columns for enlarged text, their
header wraps title and wallet, and the scrollbar follows the actual content
viewport. Fixed outer panel bounds preserve RTS hit testing. The Command page
now labels release with F1. The Garage rental section places the existing owned
vehicle chooser next to renewal/return so a newly rented vehicle is accessible
without scrolling away from those actions.

The heightmap review also exposed a housing-plot ring buried under uphill terrain.
Plot boundaries now sample the loaded heightfield at each ribbon corner, use a
bounded 64–512 segments, break at missing coverage and rebuild when resident
terrain pages change. The final native capture confirms the complete 40 m ring.
Slope, replacement-page, missing-page and date-line regressions preserve the
coverage contract without changing claim coordinates or ownership.

The whole-game document pass corrects stale direct-launch instructions, managed
versus unmanaged save semantics, Escape/F1/Space behavior, road speed/routing
limits (including the explicitly labeled unrestricted-disconnected fallback),
regional dormancy, protocol 6 and owner prediction. Historical evidence
remains explicitly versioned. It adds no claim of production balance, complete
road law, physical hardware certification or distributed MMORPG readiness.

## Verification and native review

Independent agents owned the commercial/input, road/traffic and storage/operator
work, then cross-reviewed conservation, cached validation, prediction and text
input. Verification combines pure domain invariants, actual Bevy schedules,
localhost TCP/TLS sessions, SQLite failure injection and native screenshots.
Intermediate failing or sandbox-denied attempts are diagnostic evidence; only
successful final gates count in [VALIDATION.md](VALIDATION.md).

The native review record and screenshots in
[native-v8-verification.json](native-v8-verification.json) distinguish actual UI
interactions from domain/network properties established by tests. Linux/X11 with
Mesa software Vulkan is a layout and interaction environment, not a gaming-GPU
performance result. Virtual controller/wheel tests are not physical-device
certification.

## Next structural acceptance gates

1. **Internet and identity:** run representative latency, loss, jitter and slow-client workloads; define public enrollment/recovery/moderation and secure operator delivery. Preserve protocol privacy, replay protection and failure-atomic owner activation.
2. **Durable authority scope:** extend dependency certification where invariants permit it, then measure larger global ledgers and real long-duration session churn. Develop explicit migration and shard handoff protocols before increasing aggregate limits.
3. **Production driving:** integrate a selected vehicle physics backend and authored vehicle rigs, surfaces, sound and animation. Validate collisions, trailers, assistance and multiplayer reconciliation together; geometric wheel motion is not tire physics.
4. **Complete transport gameplay:** extend individual passenger/employee behavior, manufacturable fleets, depot markets, emergency/towing activities and competitive racing. Demonstrate a balanced long career with depleted supply, failures and recovery, rather than relying on finite fixture offers.
5. **Production presentation and accessibility:** profile actual target GPUs, stream authored assets, test all supported window/platform paths, extend controller access to RTS/management, and test physical wheels, localization and assistive technology.
6. **World content at scale:** establish licensed ingestion/update pipelines, source uncertainty and temporal coverage for chosen regions. Prove historical/political/ecological correctness against the selected datasets before claiming a worldwide reconstruction.

These gates describe the next verifiable increments. The existing SDK, domain
rules, playable workflows and release evidence remain usable independently.
