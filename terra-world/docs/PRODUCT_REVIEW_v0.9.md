# Product review and dedicated improvement pass — 0.9

Review date: **2026-10-27**. The previous release review is preserved in
[PRODUCT_REVIEW_v0.8.md](PRODUCT_REVIEW_v0.8.md). This review covers the current
implementation, connected native workflows and bounded integration evidence.
The final executed commands, results and capture conditions belong to
[VALIDATION.md](VALIDATION.md); this review does not substitute development test
counts for that release record.

## Product conclusion

Terra 0.9 closes concrete gaps between regional source data, manual driving,
private warehouses, management input and authoritative hosting. Elevation and
explicit geographic features can now share a content-identified bundle across
the native client and host. Four-wheel support and actual carried load affect
the common driving model. A company can buy, store, physically distribute and
sell real inventory, while loaded recovery preserves its goods and buyer money.
In-world management and RTS have controller input paths, and slow network
receivers no longer require pretending an unsent view became a replica baseline.

The result remains a bounded reusable SDK and playable reference game.
AAA presentation, full Motor Town parity, a globally populated historical Earth
and production MMORPG capacity are development targets. No completion percentage,
player-capacity claim or indefinite economy claim follows from the test suite.
The [Motor Town matrix](MOTORTOWN_PARITY.md) and
[MMORPG foundation](MMORPG_FOUNDATION.md) define those remaining boundaries.

## Review across product perspectives

| Perspective | Current working path | Acceptance boundary |
|---|---|---|
| First-time player | Main menu → character creation → world/source/storage settings → review → loading → play. Multiplayer accepts a matching local source configuration before creating session storage. Existing managed worlds retain their chosen sources. | More guided objectives, discovery of the deeper economy, player studies, localization and longer career pacing remain. See [game flow](GAME_FLOW.md). |
| Player agency and world activity | Possession, first/third-person views, RTS orders, gathering, crafting, farming, housing, construction, supported destruction, wildlife and named community claims share the domain command boundary. Warehouse routes add work for produced goods. | The bounded reference rules are not a complete MMORPG content library, combat/endgame system or general destruction simulation. Supported sourced trees and authored structures do not make all map geometry destructible. |
| Economy and logistics | Finite personal/company/shop wallets, paid employees, public freight, private warehouse sales and internal stock transfers use distinct payers and inventories. Loaded return, hold and relief retain commitments. | Startup cost recovery, supply/demand calibration, individually scheduled commuters, insurance/tax, player markets and long-running economic balance remain game work. See [companies](COMPANIES.md). |
| Driving and traffic | Four-wheel terrain support, material grip, effective load and combined force limits reach local authority, host and owner preview. Existing directed routes, intersection queues, licenses, rentals and service loops remain connected. | Autonomous vehicles retain their waypoint/contact policy. Dynamic suspension, rigid-body impacts, articulated trailers, comprehensive traffic law and physical specialist careers remain open. See [vehicle dynamics](VEHICLE_DYNAMICS.md). |
| Geography and historical interpretation | Explicit raster-plus-GeoJSON bundles carry provenance and content identity; existing dated packs and political overlays retain revision/epoch selection. Unsupported coverage rejects. | Bundled landscapes and historical scenarios are synthetic. Regional subsets do not establish global accuracy, historical completeness, disputed-border policy, worldwide ingestion or source distribution. See [source bundles](SOURCE_BUNDLES.md). |
| Graphics and presentation | The new heightmap bundle supplies forests, fields, an orchard and specimen trees to the existing terrain/detail renderer. Terrain-aligned chassis, independently supported wheels, lamps, road cues and first/chase views communicate source and motion. | Original procedural assets and software-rendered captures do not demonstrate AAA art quality, animation, interiors, vehicle audio or target-GPU frame budgets. |
| Input and accessibility | F6/right-stick management focus selects actual visible actions, reveals scrolling controls and edits private text drafts. A bounded controller RTS cursor selects, orders, possesses and places. Existing remapping, text/FOV settings and calibrated wheel profiles remain. | Atlas polygon editing remains mouse/keyboard. Virtual input tests and native keyboard journeys do not certify physical controllers/wheels, force feedback, screen readers or localization. See [controls](CONTROLS.md). |
| Multiplayer and privacy | Protocol 7 carries private warehouse state and owner motion. Ordered queues reserve reliable-response capacity, defer optional views and converge after a stalled receiver drains. Other owners do not receive private depot ledgers. | Finite TCP/TLS loopback conditions do not establish Internet behavior, combat rewind, production concurrency or cross-authority handoff. See [replication](NETWORK_REPLICATION.md). |
| Host trust and operations | A trusted maintenance API provisions a dormant principal while guest admission stays closed, and commits identity, ownership and audit together when regional storage is attached. Existing suspension/rotation remain available. | The embedding game must authenticate operators, privately deliver capabilities and provide real-person enrollment, recovery, moderation and external audit. See [operations](OPERATIONS.md). |
| Persistence and scale | Commercial escrow, stock reservations, dormancy and saved travel proof survive bounded restore. Existing regional paging, unchanged environmental certificates and exact equal-page reuse remain intact. | One authority/writer, aggregate collection caps, maintenance barriers and complete restore validation remain. Archived performance probes are not current production capacity evidence. See [scaling](SCALING.md). |
| SDK integration and compatibility | Core support math, source import, domain commerce, host authority and native presentation remain separated. New data defaults preserve compatible older saves under pinned catalog/source identity. | Save schema 1 is not permission to exchange every new variant with an older executable. Source changes need a new world or an explicit validated migration; production physics, content, services and deployment remain game-owned. |

## Findings closed during review and polishing

| Finding and consequence | Correction and concrete implementation/evidence path |
|---|---|
| A height raster alone could not communicate forests, fields, roads or source attribution for those features. | The optional semantic layer imports a bounded explicit GeoJSON subset, clips and grounds its geometry, retains tags/provenance and hashes semantic bundle contents. See [heightmap provider](../crates/terra-stream/src/heightmap.rs) and [feature normalization](../crates/terra-stream/src/heightmap_features.rs). |
| Multipart expansion or tile clipping could multiply large attribute maps; concave clipping could create invalid filled geometry. | Expanded property-count/text budgets are reserved before cloning attributes. Geometry clips first; unsupported touching/disconnected rings reject rather than fill the gap. Focused importer regressions exercise both boundaries. The accepted subset and limits are in [SOURCE_BUNDLES.md](SOURCE_BUNDLES.md). |
| A regional bundle was not a complete native join/host workflow. | The host admits only covered tiles and validates complete spawn coverage; native Join accepts and validates a fixed local WorldConfig. Matching seed, source level, resolution and content revision are required. [MULTIPLAYER.md](MULTIPLAYER.md) provides the correct separate host/client commands; no source download is implied. |
| One body-level terrain estimate and cargo-only inspection left wheel support, materials, passengers and trailer mass inconsistent. | [Core support sampling](../crates/terra-core/src/vehicle_ground.rs) and [transport mechanics](../crates/terra-sandbox/src/transport/mechanics.rs) derive actual support and load. Manual authority and replay use the shared model; [vehicle-ground tests](../crates/terra-sandbox/tests/vehicle_ground.rs) exercise load transitions, fuel, braking and save continuity. |
| Visual wheels followed a rigid chassis without independent terrain travel. | [Vehicle presentation](../crates/terra-demo/src/vehicle_presentation.rs) samples each visible axle and applies bounded critically damped springs. Missing coverage preserves supported travel, true pause freezes it, and presentation does not mutate authority. This closes a visual gap, not dynamic suspension physics. |
| Private depot inventory was storage without a complete market/distribution loop. | [Depot markets](../crates/terra-sandbox/src/transport/company/market.rs) quote-check real buys/sales; private sale routes reserve buyer money at loading and pay only after physical delivery. Internal transfers pay no public fare. Public freight keeps its separate public stock contract. |
| Later deposits or trade could consume space needed to return or deliver loaded warehouse goods. | Source refund and destination arrival reservations count against mixed-item capacity. Returning releases only abandoned destination space. [Warehouse tests](../crates/terra-sandbox/tests/company_depots.rs) cover caps, partial loads, stale quotes, refunds, relief, cold restore and atomic rejection. |
| Management/RTS input still depended on pointer-only paths, and a text editor could mishandle concurrent state. | [Command input](../crates/terra-demo/src/command_input.rs) operates enabled visible buttons, isolates private drafts and preserves retry on a full queue or changed field. Controller selection/orders/placement use existing ownership and terrain checks. Normal physical typing and Enter also work in the on-screen keyboard. |
| Entering management before terrain arrived could leave the camera below the loaded ground; focus could reopen an old panel. | Camera grounding now runs with presentation/source synchronization independently of input focus. Management entry resets selection to the current panel; Fleet visibility updates precede interface layout. These protect early-loading and repeated panel-switch journeys. |
| Fixed footer spacing could clip enlarged guidance and leave panels or floating HUD elements overlapping it. | [Shared interface layout](../crates/terra-demo/src/interface.rs) now derives footer height from 96px × text scale and reserves that height + 26px in the world viewport. Side panels, atlas captions, scroll tracks and floating driving HUDs follow that reservation. `interface::layout_tests::scaled_footer_reserves_the_same_world_panel_and_floating_hud_bounds` passed through 100% → 150% → 100% text scaling, including world hit-test exclusion. Native confirmation at 1100×700 with 150% text shows the complete footer and separate inspector boundary; see [VALIDATION.md](VALIDATION.md). |
| Keyboard focus could reach a clipped action without revealing it because ordinary nested nodes also carry scroll state. | Focus reveal now follows the ancestor's actual vertical overflow policy and skips plain pages and horizontal-only wrappers. A nested regression reproduced the zero-scroll failure, then passed with 69 logical pixels of movement at 2× DPI; intermediate containers stay unchanged. The final native Driving inspection confirms the action becomes visible. |
| Optional snapshots could advance a baseline before queue admission; reliable commands could be admitted without response space. | [Server queue handling](../crates/terra-net/src/server.rs) defers optional baseline advancement, coalesces a pending latest view and reserves control headroom. Reliable batches wait for their maximum response budget. Resync retries are bounded; failed sends cannot create phantom pending state. [Network-condition tests](../crates/terra-net/tests/network_conditions.rs) verify convergence and exactly-once mutation. |
| A fresh closed-guest host lacked a trusted provisioning path. | `Server::provision_principal` and its explicit-endpoint variant validate and commit a dormant principal before publication. [Operator tests](../crates/terra-net/tests/operator_controls.rs) exercise capacity, unsafe endpoint and actual SQLite lock rejection without leaking IDs, capabilities or ownership. The API is not a public account service. |
| Current docs could imply obsolete protocol/input limits or treat old measurements as this release. | Current contracts identify wire protocol 7 and TLS ALPN `terra-world/7`; the native source command is corrected, visual springs are distinguished from dynamic suspension, and archived scaling evidence remains explicitly versioned. Final gate counts are centralized in [VALIDATION.md](VALIDATION.md). |

## Connected evidence and what it establishes

### Native source join and company commerce

The current native journey entered the actual Multiplayer Join flow with the
matching local heightmap configuration and connected to a loopback authoritative
host. The on-screen keyboard accepted ordinary typing and Enter to create
**ValleyFreight**. Company registration, funding, depot opening and market trades
then crossed the real UI/domain/network boundary.

| Committed step | Personal credits | Company credits | Private depot bread |
|---|---:|---:|---:|
| Initial principal | 250.00 | — | — |
| Register company for 20.00 | 230.00 | 0.00 | — |
| Transfer 50.00 into company | 180.00 | 50.00 | — |
| Open depot for 25.00 | 180.00 | 25.00 | 0 |
| Buy one bread for 2.90 | 180.00 | 22.10 | 1 |
| Sell that bread for 1.40 | 180.00 | 23.50 | 0 |

The inspected durable state retained personal **180.00**, company **23.50** and
the empty bay. The transaction loses the actual shop spread; pressing buy/sell
does not create wealth. This proves one connected native commerce journey,
including source admission and typed company naming. It does not prove every
menu branch, a controller device, a long-running career or a public service.
Final captures and execution conditions are recorded in [VALIDATION.md](VALIDATION.md).

### Finite physical warehouse career

The deterministic domain fixture in
[company_depots.rs](../crates/terra-sandbox/tests/company_depots.rs) spans six
sessions, 24 completed deliveries and seven save/resume checkpoints. It
physically transfers private stock before selling it, encounters an exhausted
fuel supply, buys finite roadside service, cancels/retries a transfer and
recovers from unfunded payroll through a real owner deposit.

The measured **2,638.45 seconds** include dormant and off-duty intervals. Twelve
internal transfers earn no revenue; twelve shop deliveries sell **96 water**.
Sales total **28.80 credits**, wages **8.91** and roadside payment **18.00**,
leaving **1.89 credits** of operating cash change excluding owner transfers.
Registration, hiring, bays and harvest acquisition are outside that operating
figure. Across the fixture, **13,900 credits**, **224 water** and **40 bread**
remain conserved when recorded consumption is included. An exhausted source
ends in **WaitingGoods** with no idle income or payroll.

This is useful conservation, continuity and recovery evidence. Its isolated
flat corridor, finite starting harvest and short routes do not establish
profitable full-career balance or rendered-terrain navigation. Exact setup and
reproduction are in [COMPANIES.md](COMPANIES.md).

### Delayed and stalled authoritative sessions

The shared network fixture uses actual TCP and TLS connections with an opaque
bounded relay: **25 ms one-way scheduled delay**, **0–15 ms ordered jitter**,
writes of at most **997 bytes**, and **128 KiB buffering per direction**.
It pauses a receiver until real socket backpressure occurs while a second
client continues receiving service, then verifies exact private convergence.
The shared journey includes a reliable craft, save/restart and resume, and
rejection of its already-accepted sequence. A separate reliable-admission
regression proves that a craft waits without mutation until response capacity
is available, then executes exactly once.

The **512 principal actor records** include **510 simple kinematic stress
records**. They are neither 512 living NPCs nor 512 concurrent players.
Application write fragmentation is not a simulation of Internet packet loss
or reordering. The fixture's configuration and assertions are documented in
[NETWORK_REPLICATION.md](NETWORK_REPLICATION.md).

### Source identity, compatibility and release evidence

The new semantic bundle contains original synthetic terrain/land-cover content
near a real GPS coordinate. It is not observed Munich geography. New launcher
heightmap worlds select `manifest-v2.json`; older compatible profiles retain
their original raster-only source. Semantic identity includes canonical
manifest, PNG and GeoJSON bytes. Legacy raster-only manifests retain their
declared revision for compatibility, so publishers must change that revision
when changing the legacy raster.

Domain save schema **1** and regional schema **1** remain separate from wire
protocol **7**. Additive defaults preserve compatible existing state; new route
variants still require a compatible reader. Changing a source identity is not
an automatic save migration. The separately scoped
[0.9 executable admission smoke](server-admission-v0.9.json) records its own
binary/fixture identity; it must not be substituted for the final source gate.

Native captures use the recorded Linux X11/software Mesa environment. They
verify actual presentation and workflows, not gaming-GPU performance.
[VALIDATION.md](VALIDATION.md) identifies the release commands, outcomes and
capture conditions. Versioned 0.8 reports and probes remain historical evidence;
they are not rerun 0.9 performance measurements.

## Remaining structural and product priorities

These are substantive acceptance requirements for the larger product, not
features considered complete by this release:

1. **Production driving and physical work.** Integrate or develop a measured
   suspension/tire/contact model, articulated trailer bodies, towing and cargo
   equipment before claiming physical Motor Town parity. Exercise slopes,
   bridges, loading, recovery and contested traffic with representative vehicle
   classes. Autonomous traffic needs an explicit migration from its current
   waypoint policy; improving manual support alone does not supply that.
2. **AAA visual and audio production.** Establish a coherent asset pipeline for
   terrain materials, vegetation, people/animals, vehicles, interiors, animation,
   lighting and sound. Measure streaming, draw work, memory and frame time on
   target GPUs across day/night, weather and crowded scenes. Software screenshots
   and bounded source meshes are useful inspection evidence, not those budgets.
3. **Sustained player goals and accessible operation.** Teach the gathering →
   production → shop/fleet → property/community loop through discoverable tasks.
   Tune finite acquisition costs, wages, routes and demand against longer
   careers, including failure recovery. Test physical controllers/wheels,
   supported window sizes and assistive needs; complete atlas input coverage,
   localization and a broader accessibility strategy.
4. **World-scale and historical data delivery.** Choose licensed datasets,
   spatial indexes, distribution/cache policy, revision updates and explicit
   historical uncertainty/disputed-border rules. General GIS topology, vertical
   datums, gaps and heterogeneous source conflicts need deliberate contracts.
   Regional offline bundles and dated fictional packs cannot stand in for
   planet-wide source coverage.
5. **Public multiplayer operations.** Build authenticated operator tools and
   real-person identity/recovery around the trusted APIs. Add private capability
   delivery, abuse/moderation policy, certificate operations, observability and
   representative Internet acceptance. Run longer faults/soaks with meaningful
   populated-world workloads before making availability or concurrency claims.
6. **Larger authority and persistence architecture.** Define ownership and
   transactional handoff across regions/authorities, schema/source migrations,
   broader dependency proofs and recovery procedures before increasing global
   caps. Measure cold/warm traversal, reconnect churn, save pauses, queue bounds
   and memory plateaus on the resulting architecture. Current paging is useful
   within one bounded authority; it does not supply a distributed MMORPG.

The current release makes the reference systems cooperate more reliably and
exposes the remaining work with concrete contracts. Integration teams can reuse
the implemented boundaries while choosing their own content, realism,
deployment and player-experience requirements.
