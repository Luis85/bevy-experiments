# Terra World 0.12 - Open gaps report

Baseline audit: **0.12.0**, 28 October 2026.

This is the original independent 0.12 audit added to the project during the 0.13 improvement pass. It describes the accepted 0.12 source and evidence, not the completion status of later releases. The unchanged [PDF report](reports/Terra-World-0.12-Open-Gaps-Report.pdf) is included. Consult [the 0.13 follow-up](REVIEW_GAPS_v0.13.md) for implementation and verification status. The 0.12 [product review](PRODUCT_REVIEW_v0.12.md) and [validation record](VALIDATION_v0.12.md) preserve the original current-document references.

Audited source ZIP SHA-256: `80683be326676e90d6b920d463456ac38ca8991422284b01f77786e0b1bfffe9`.

## Findings

The documentation supports a verified regional SDK and playable reference game. It also identifies substantial unfinished product behavior, content production and deployment qualification. Passing the 0.12 gates closes their stated cases; it does not establish a finished Windows game, a sustainable long career, a production Internet service or a populated worldwide simulation.

Make the next product pass a Windows-qualified regional career slice: correct current documentation, demonstrate a viable player strategy, test the full first-session and returning-player journeys, and make navigation failures recoverable. Run public-service work in parallel only if Internet hosting is part of the next release. Defer distributed/global architecture until a measured product requirement justifies it.

## Scope and priority

Read-only documentation audit of 30 current top-level guides, the root README/changelog, selected crate references and retained 0.12 acceptance records. Three subagents independently reviewed gameplay/economy, content/SDK and operations/release. The 30 guides plus five release/reference files were byte-compared with the delivered ZIP (35 matched inputs); the ZIP SHA-256 matches the delivered artifact. No game tests were rerun and no new playtest, hardware qualification, external competitor research or broad code audit was performed. A narrow protocol constant read resolved a conflicting documentation claim. The release source and ZIP were not changed.

- **P1:** A gate before the named audience or promise. Conditional P1 service work does not block a deliberately private regional SDK demonstration.
- **P2:** Planned product depth, content or accessibility work. Promote when the corresponding feature or audience is committed.
- **P3:** Conditional expansion. Decide the product scope before allocating major engineering effort.

## Accepted evidence and its limits

### Automated acceptance

20 gates; 747 distinct passing Rust tests; zero failed/ignored; 327 unchanged source inputs.

Establishes the executed correctness checks, not complete product readiness.

Source: [S01](#source-index).

### Final native acceptance

Three clean sessions; 14 captures; 12 named exact save/reopen comparisons; 1440 x 900 and 1100 x 700 at 150% text.

Linux X11/Xvfb and llvmpipe software Vulkan; selected keyboard/mouse flows, not Windows or physical-device qualification.

Source: [S01,S02](#source-index).

### Paired careers

Each career: 24 deliveries in six fixture sessions; 2,967.45 physical seconds (about 49.5 minutes). Known-cost results: -84.69 / -161.49 credits.

Each includes one forced 15-credit callout; the purchased-stock variant buys retail and sells wholesale. These fixtures do not prove every strategy is unprofitable.

Source: [S03,S04](#source-index).

### Traffic recovery

The original saved-load fixtures complete in 34.0 seconds and 49.5 seconds.

Two closed regressions. No general deadlock or shortest-path guarantee.

Source: [S05](#source-index).

### Live recovery workload

16 local TLS clients; 182 bodies (80 owned); 25 source chunks; 1,024 cold regions; 4,840 steps in 131.962 wall seconds.

Cold regions contain 4,096 depleted-resource records outside local prepared terrain. They are not 1,024 inhabited or traversable regions.

Source: [S06](#source-index).

### Host timing and memory

Advancing update p95 46.691 ms, p99 57.483 ms, maximum 91.151 ms; durable-ack p95 108.879 ms; process RSS 36.04 to 100.27 MiB.

Shared host/client process and finite run. No guaranteed 20 Hz deadline, isolated server RSS or long-duration memory plateau.

Source: [S06,S07](#source-index).

### Asset and source proof

Five validated wildlife models; scene admission/fallbacks; two authored connected bundles with exact 81-tile coverage and offline roundtrip.

Estimated scene payload is not actual GPU allocation. Authored data is not licensed global or historical coverage.

Source: [S08,S09](#source-index).

## Open gaps at the 0.12 baseline

### G01 — Current documentation is not fully consistent

**Priority:** P1 · **Gate:** External SDK handoff  
**Classification:** Confirmed documentation defect; minor navigation/version drift  
**Suggested owner:** Maintainer / technical writer · **Dependencies:** None

**What works.** The runtime, main integration guide and accepted release record agree on wire protocol 9 and TLS terra-world/9. Finances is implemented and the final interface has six company sections.

**Open gap.** The current protocol reference in crates/terra-net/README.md still says protocol 7. GAME_FLOW and COMPANIES omit Finances from older five-section lists. GAMEPLAY and SETTLEMENTS_AND_ECONOMY still introduce their unarchived guides as 0.9; CLIENT_PREDICTION mixes historical introduction wording with current instructions.

**Product impact.** The protocol error can mislead an integrator into an obsolete handshake. The other items create avoidable navigation and version ambiguity; they are not missing runtime functionality.

**Completion criteria.** Correct the current protocol and navigation references, distinguish introduction history from current applicability, and check all current compatibility statements against one canonical protocol/ALPN/save/schema source. Preserve archived versions. Treat the protocol correction as the immediate item; the wording/navigation cleanup is lower severity.

Sources: [S10](#source-index), [S11](#source-index), [S03](#source-index), [S12](#source-index), [S13](#source-index), [S14](#source-index).

### G02 — Windows delivery and installation are unqualified

**Priority:** P1 · **Gate:** Windows player release  
**Classification:** Platform validation and productization gap  
**Suggested owner:** Release / platform engineering · **Dependencies:** Supported Windows and graphics baseline

**What works.** The source release contains the complete project and assets, with a pinned toolchain. The frozen Linux native executable passed the documented confirmation sessions.

**Open gap.** The delivered ZIP is a source package. Windows and macOS native builds were not exercised; the documented setup is a development build with Linux prerequisites. There is no retained clean-machine Windows install/run/update acceptance record.

**Product impact.** A Windows user cannot treat this deliverable as a qualified installable game. The documentation does not demonstrate that a Windows port is broken; it leaves that platform unverified.

**Completion criteria.** Declare supported Windows versions, architecture and graphics requirements. Build a distributable package with assets and prerequisite handling. Exercise clean-machine startup, creation, normal play, save/reopen, update and removal using the shipped executable; retain hashes and a troubleshooting guide. Decide signing and update-channel policy for the intended distribution.

Sources: [S01](#source-index), [S02](#source-index), [S15](#source-index).

### G03 — Sustainable commercial progression is not demonstrated

**Priority:** P1 · **Gate:** Playable economic career  
**Classification:** Balance and long-duration validation gap  
**Suggested owner:** Game design / economy / QA · **Dependencies:** Intended player strategies, content supply and route reliability

**What works.** Cash categories, owner funding, known personal support, physical delivery and value conservation reconcile. The acquisition cost difference between contributed and purchased goods is now visible.

**Open gap.** The two careers finish at -84.69 and -161.49 credits after known owner costs. These are about 49.5 physical minutes split into six fixture sessions, not six long player sessions. Harvest labor, vehicle capital and prepaid inputs remain unpriced where unknown.

**Product impact.** The fixtures prove accounting and recovery, but do not establish which intended strategies support satisfying ongoing progression. Their forced 15-credit callout and retail-to-wholesale loss must not be generalized into a claim that all careers fail.

**Completion criteria.** Specify viable beginner and advanced strategies, progression targets and accepted scarcity/failure conditions. Demonstrate them over a declared multi-session duration with setup, acquisition, wages, fuel and recovery included, without developer grants after the starting budget. Keep poor strategies and exceptional costs visible. Explicitly define or exclude capital/labor valuation; do not tune prices merely to make a recovery fixture positive.

Sources: [S03](#source-index), [S04](#source-index), [S16](#source-index).

### G04 — Continuing goals need content depth and player validation

**Priority:** P1 · **Gate:** Sustained player experience  
**Classification:** Missing content and usability evidence  
**Suggested owner:** Game design / UX research / content · **Dependencies:** G03 balance, G05 navigation, target audience

**What works.** Suggested, Supply, Home, Community and Road guidance derives from actual state, urgent needs, permissions and accepted work. Actions use normal authority and displayed-intent checks; progress survives save/reopen.

**Open gap.** The guidance guide explicitly leaves long-career pacing, authored quest/content libraries, localization and player-study evidence to further development. Its bounded local search is not a global recommendation or optimum-route system.

**Product impact.** A correct next action can still leave a first-time or returning player unsure why progress stalled, what is worth doing next, or how to recover from depleted stock, missing facilities and changed permissions.

**Completion criteria.** Define the first-session and longer-career journeys and their intended duration. Supply the content needed for those journeys. Observe new players completing supply, home/community and road/company paths without developer coaching, including setbacks. Record stalls, misunderstood costs, abandonment and recovery, then close the observed problems with measurable acceptance targets.

Sources: [S17](#source-index), [S16](#source-index), [S18](#source-index).

### G05 — Navigation and autonomous recovery remain local

**Priority:** P1 · **Gate:** Denser or larger playable regions  
**Classification:** Bounded capability and scenario-coverage gap  
**Suggested owner:** Navigation / simulation / QA · **Dependencies:** Supported topology, source-road quality and body budgets

**What works.** Shared bounded handling, source/body support, directed road rules, junction permits and temporary detours recover the two retained traffic fixtures while preserving cargo and continuation.

**Open gap.** Walking prunes admitted existing waypoints rather than planning arbitrary new obstacle corners. The roughly 77 m native market approach retained a 261.8 m safe route around a parked pickup. Impossible or unsupported corridors may hold indefinitely; full lane/turn/signal laws and general congestion recovery are absent.

**Product impact.** More crowded or imported regions can produce inefficient travel or persistent blockage even while all movement remains safe. The player needs a useful recovery path when automation cannot find progress.

**Completion criteria.** Declare supported geometry and obstruction limits. Exercise narrow roads, blocked junctions, alternate exits, moving obstructions, missing/restored terrain and save/resume. Require safe physical progress within an agreed scenario budget or an explicit, usable recovery action that preserves goods and escrow. Add new corner-routing behavior where measured cases require it; never require movement through impossible geometry.

Sources: [S05](#source-index), [S16](#source-index), [S19](#source-index), [S20](#source-index).

### G06 — Shared communities lack a complete lifecycle

**Priority:** P1 · **Gate:** Long-lived shared worlds  
**Classification:** Explicit missing player-facing capability  
**Suggested owner:** Gameplay / governance / persistence · **Dependencies:** Membership policy, identity recovery and migration rules

**What works.** Invitations, roles, current-role authorization, shared housing/workbenches and atomic transfers into factions are implemented. Public works consume real resources and funds.

**Open gap.** Faction transfer is one-way. There is no leave/kick flow, leadership succession, later territory ownership transfer or general treasury withdrawal. Public stock and treasury contributions are restricted and irreversible in the reference policy.

**Product impact.** A group can acquire durable assets but lacks a complete supported transition when members leave or leadership must change. Irreversible public contributions may be intentional; unrestricted withdrawal is a product decision, not an automatic fix.

**Completion criteria.** Define outcomes for homes, plots, crops, invitations, territories and public funds during departure, removal, succession and ownership changes. Implement the minimum intended transitions atomically, including offline members and an unavailable leader, and prove exact permissions/value after save/resume. Clearly retain any deliberate irreversible contributions.

Sources: [S12](#source-index), [S13](#source-index), [S21](#source-index).

### G07 — Real rendering performance and memory are unmeasured

**Priority:** P1 · **Gate:** Minimum-spec or frame-rate promise  
**Classification:** Hardware qualification gap  
**Suggested owner:** Rendering / performance QA · **Dependencies:** Target devices and representative production content

**What works.** Quality profiles, bounded source mesh work, aggregate validated actor admission, loading/retirement reservations, fallbacks and timing/scene diagnostics exist.

**Open gap.** Final native evidence uses software Vulkan. Scene payload estimates exclude actual driver/allocator behavior and full CPU/GPU residency. Direct game-owned scene overrides are unmetered. Frame intervals and CPU counters are not GPU timestamps.

**Product impact.** Passing import budgets cannot establish a minimum GPU, frame-time tails, upload hitches, memory reclamation or long-session stability. The observed 9.6 MiB estimate must not become a VRAM claim.

**Completion criteria.** Choose frame-time and memory targets, then profile release builds on selected integrated/discrete GPUs. Include busy streets, player overlays, weather/night, cockpit views, region transitions, model churn and storage barriers. Record CPU/GPU distributions and actual residency/loading peaks. Include custom overrides in the product budget or require validated metadata for them.

Sources: [S07](#source-index), [S08](#source-index), [S22](#source-index), [S01](#source-index).

### G08 — Device breadth and broader accessibility need qualification

**Priority:** P2 · **Gate:** P1 for each claimed device/accessibility path  
**Classification:** Validation gaps and selected unsupported features  
**Suggested owner:** UX / accessibility / platform QA · **Dependencies:** G02, supported devices and accessibility commitments

**What works.** Remapping, keyboard focus, 100-150% text, controller menu/text/RTS/building flows, wheel calibration and safe neutral rearming have bounded implementations and tests.

**Open gap.** Physical controllers/wheels, screen-reader behavior and localization remain unqualified. Complete end-to-end keyboard-only qualification remains incomplete. Atlas polygon editing remains mouse/keyboard. Shifters, force feedback, raw HID and platform clipboard integration are outside the current path; identical wheel-device ordering can require reselection.

**Product impact.** Virtual input success and two window sizes do not establish real-device usability or assistive access. Players can encounter a required task outside their supported input path.

**Completion criteria.** Publish a device/platform/task matrix. Exercise creation, delivery, servicing, management and save/reopen on every claimed path, including focus loss, reconnect, calibration and device-order changes. Validate contrast, focus, text expansion, motion comfort and recoverable errors with representative users. Decide explicitly whether controller atlas editing, localization, shifters and force feedback are in scope.

Sources: [S23](#source-index), [S11](#source-index), [S17](#source-index), [S21](#source-index).

### G09 — Production character, vehicle and audio content is incomplete

**Priority:** P2 · **Gate:** Polished consumer presentation  
**Classification:** Content production and workflow capability gap  
**Suggested owner:** Art / animation / technical art / audio · **Dependencies:** Art direction, roster, interaction requirements and G07

**What works.** Immutable validated GLB packs, per-binding animation policies, fallback, pause-safe blending and the original five-species wildlife example form a working asset handoff. Character customization remains intact.

**Open gap.** A game-ready human/vehicle/environment art set, detailed cabins, mirrors, animated passenger boarding and rendered seat occupants, finished vehicle/surface audio, equipment sockets and broader animation/retargeting workflows are not delivered by the wildlife example. The GLB profile intentionally rejects several advanced features.

**Product impact.** The pipeline is usable infrastructure, but does not yet demonstrate a complete production character/customization or vehicle-interaction workflow. Preserved procedural customization is not proof of imported-human retargeting.

**Completion criteria.** Deliver one representative human, vehicle and passenger/interaction slice with a tested authoring/export preset, licensing, LOD/material budgets and audio plan. Verify customization, equipment/cargo attachment, first-person hiding, animation transitions and night/rain/cockpit use on target hardware. Extend the supported importer only for required assets; unrestricted glTF is not a default goal.

Sources: [S08](#source-index), [S22](#source-index), [S24](#source-index), [S18](#source-index).

### G10 — A production regional data pipeline is still game-owned

**Priority:** P2 · **Gate:** P1 before a real-world shipping-region claim  
**Classification:** Explicit data/content capability gap  
**Suggested owner:** Geospatial / content engineering · **Dependencies:** Target region, source rights, fidelity and activities

**What works.** Prepared raster/semantic import, provenance, connected coverage, mixed-resolution seam checks, immutable identity and a two-bundle 81-tile authored example are implemented.

**Open gap.** The docs call for reproducible extract ingestion, relation assembly, topology/biome classification and source-specific attribution. Dense grayscale inputs need preprocessing for NoData; datum conversion, complex polygon relations, bridge-deck/foundation reconstruction and real historical coverage are not inferred by the importer.

**Product impact.** A verified package establishes its admitted bytes and contract, not geographic completeness, accuracy or gameplay-ready roads. Public development APIs are not a shipping-world availability guarantee.

**Completion criteria.** Prepare one licensed regional vertical slice from pinned raw extracts. Document rights, datum/NoData, topology, bridges and clipping policies. Produce reproducible packages and audited difficult fixtures. Traverse actual seams, roads, buildings and missing-coverage edges in native and host paths. A worldwide dataset is not required to close this regional gap.

Sources: [S09](#source-index), [S25](#source-index), [S26](#source-index), [S22](#source-index).

### G11 — Content delivery and trusted publication remain external

**Priority:** P2 · **Gate:** P1 for downloadable world content  
**Classification:** Distribution/lifecycle capability gap  
**Suggested owner:** Launcher / content delivery / platform · **Dependencies:** G10 publishing policy, hosting and supported clients

**What works.** Offline inspect/export/verify/install tooling preserves source identity, checks hashes and rejects unsafe or existing destinations. Failed installation preserves existing releases.

**Open gap.** Required source files must already exist on each machine; the gameplay connection does not download them. Publisher authenticity, release discovery, interrupted transfer, installed-version selection and content rollback need a product-level workflow. Content hashes are not publisher signatures.

**Product impact.** Joining a valid remote world can still require manual preparation and mismatch troubleshooting. An immutable package is not itself an authenticated distribution channel.

**Completion criteria.** Choose a trusted release channel and a client-facing prerequisite flow. Obtain the selected release, resume interrupted transfer, verify trusted identity, install safely and launch only with the exact expected source or a recoverable explanation. Demonstrate that tampered or interrupted content never becomes selected runtime data. Signing is one possible mechanism; choose trust appropriate to the deployment.

Sources: [S09](#source-index), [S25](#source-index), [S21](#source-index).

### G12 — Persistent worlds lack a supported content-upgrade path

**Priority:** P1 · **Gate:** Updates to persistent customer worlds  
**Classification:** Explicit migration capability gap  
**Suggested owner:** Persistence / content / gameplay architecture · **Dependencies:** Versioned source/catalog releases and G16 recovery policy

**What works.** Strict source/catalog identity and schema checks prevent silent replacement of saved authority. New-world creation and no-overwrite recovery are valid supported paths.

**Open gap.** Tools do not migrate saved worlds between new source extents, epochs, catalog definitions or upstream feature IDs. The game must define conflicts between new geometry and existing structures, ownership, resource tombstones and active obligations. Shared save-version numbers do not certify arbitrary mixed executables.

**Product impact.** A content update can be safely rejected yet still leave customers without a supported way to continue their existing world. Changing a revision label cannot solve the conflict safely.

**Completion criteria.** Ship one versioned old-to-new migration with mappings, conflict policy, dry-run diagnostics, rejection and backup/rollback. Preserve money, inventory, property, depletion, cargo and account sequences. Include changed source IDs and new geometry conflicts. If worlds stay permanently pinned instead, state that product policy and retain the required old content and executable support.

Sources: [S09](#source-index), [S24](#source-index), [S26](#source-index), [S27](#source-index), [S01](#source-index).

### G13 — Public accounts, operator access and abuse handling are missing

**Priority:** P1 · **Gate:** Public multiplayer service  
**Classification:** Explicit production-service integration gap  
**Suggested owner:** Backend / identity / security operations · **Dependencies:** Public/private hosting decision and identity policy

**What works.** Verified TLS, private resume capabilities, replay/privacy checks, exact-principal provisioning, suspension, rotation and resume-only admission are present. With regional storage, administrative changes commit before live policy changes.

**Open gap.** The host exposes trusted APIs, not a real-person enrollment/recovery/moderation service or authenticated operator interface. Guests are enabled by default; resume-only is a startup policy. The 128-entry simulation-time audit is diagnostic, not independently retained security history.

**Product impact.** A public deployment must supply secure enrollment, operator authorization, private capability recovery/delivery and abuse policy. This is an integration gap, not evidence of a TLS bypass or a known exploit.

**Completion criteria.** Define identity-to-principal mapping, enrollment/recovery, operator roles, private credential delivery, revocation and abuse policy. Test closed admission after restart, successful/failed rotation, account recovery and audit retention while preserving property and sequences. A trusted invitation-only product may satisfy this with a narrower design than public registration.

Sources: [S28](#source-index), [S21](#source-index), [S29](#source-index).

### G14 — Internet play quality and sustained capacity are unproven

**Priority:** P1 · **Gate:** Internet or concurrent-player claims  
**Classification:** Network and load qualification gap  
**Suggested owner:** Networking / performance QA / hosting · **Dependencies:** Population target, deployment hardware and service levels

**What works.** Recipient-private deltas, bounded queues, resync, owner display reconciliation and finite delayed/fragmented localhost TCP/TLS cases preserve authority and continuation.

**Open gap.** The relay fixture uses two localhost clients with 25 ms one-way delay and 0-15 ms ordered jitter, not general Internet loss/reordering. Owner replay is bounded to 250 ms/32 frames. TCP head-of-line blocking remains. The final 16-client recovery workload is short and local.

**Product impact.** Smooth remote driving, reconnection experience, long-session memory stability and a supported player count cannot be inferred from these correctness cases.

**Completion criteria.** Agree latency/jitter/loss/disconnection profiles, population mix and session duration. Exercise the actual game/host on declared deployment hardware with representative certificates. Measure visible corrections, command completion, queues, bandwidth, disconnects and server-only memory through a sustained soak. Retain exact value/privacy checks. Select transport changes from measured failures; a rewrite is not automatically required.

Sources: [S30](#source-index), [S14](#source-index), [S06](#source-index), [S29](#source-index).

### G15 — Storage transitions still pause authoritative simulation

**Priority:** P1 · **Gate:** Continuous online simulation promise  
**Classification:** Explicit architecture limitation and latency qualification gap  
**Suggested owner:** Persistence / simulation performance · **Dependencies:** Agreed pause/ack/tick targets and G14 workload

**What works.** A bounded worker, generation guards and durable acknowledgements preserve authority on failed writes and retries. Presentation and host polling can remain responsive.

**Open gap.** Mutations wait while a storage transition owns authority; main-thread cloning and installation still cost time. Current ledgers and dependent cold records remain validated. Measured advancing-update p99 is 57.483 ms and maximum 91.151 ms; this does not guarantee every 20 Hz deadline.

**Product impact.** Responsive polling is not uninterrupted simulation. The 2,119 pending-storage calls are a call count, not elapsed pause duration; the 108.879 ms durable-ack p95 includes local fixture/storage behavior.

**Completion criteria.** Measure total authority pause, submission, worker, installation and command acknowledgement separately under representative gameplay and churn. Define allowable tails. If they fail, reduce transition costs or certify additional independent transaction classes while retaining differential full-merge checks and complete validation of untrusted state.

Sources: [S06](#source-index), [S07](#source-index), [S27](#source-index), [S28](#source-index).

### G16 — Recovery tools need a complete operational recovery policy

**Priority:** P1 · **Gate:** Hosted persistent worlds  
**Classification:** Deployment recovery and lifecycle gap  
**Suggested owner:** Operations / persistence / release · **Dependencies:** Deployment design, G11 content retention and G12 upgrades

**What works.** Verified online backup, recovery to a new destination, corruption rejection and authenticated continuation were executed. Backup is not missing; its snapshot semantics and source matching are explicit.

**Open gap.** Retention/offsite policy, backup confidentiality, complete source/catalog/configuration retention, recovery ownership, formal migrations and rehearsed service recovery remain deployment work. Saves do not bundle all source assets.

**Product impact.** A valid database copy does not establish recovery of a deployed service and its clients. The recorded 42.378 ms copy receipt is not a production recovery-time objective; abrupt-process tests are not physical power-loss qualification.

**Completion criteria.** Agree recovery-point and recovery-time objectives (RPO/RTO). Assemble a recovery kit with matching world/content/configuration dependencies and secure credential handling. Rehearse restoration onto a clean host, returning clients, migration and rollback; measure complete service recovery and acceptable lost work. Record who performs and verifies each step.

Sources: [S28](#source-index), [S27](#source-index), [S21](#source-index), [S06](#source-index).

### G17 — Diagnostics do not yet form production observability

**Priority:** P1 · **Gate:** Maintained hosted service  
**Classification:** Explicit diagnostic boundary; operational recommendation  
**Suggested owner:** Operations / support / telemetry · **Dependencies:** Hosting, privacy/retention policy and incident process

**What works.** In-game diagnostics and structured scene, source, timing, replication and persistence counters are available. Telemetry excludes private capabilities and complete world state.

**Open gap.** Timing windows retain up to 240 samples. The documented telemetry file is written on graceful exit and can be absent after a crash. Continuously retained metrics, crash collection, alerts, external audit retention and incident triage are not supplied by that export.

**Product impact.** A deployed failure may remove the evidence needed to diagnose it; bounded in-memory diagnostics cannot establish service detection or support readiness.

**Completion criteria.** Define secret-safe crash reporting, periodic/continuous host metrics and retained deployment logs. Add actionable detection for failed checkpoints, sustained queue pressure, repeated source failure and capacity exhaustion; cover account abuse through the chosen security pipeline. Rehearse one incident from detection through recovery and confirm useful evidence survives abrupt termination.

Sources: [S07](#source-index), [S28](#source-index), [S21](#source-index).

### G18 — Large populated or worldwide operation needs a scope decision

**Priority:** P3 · **Gate:** P1 only for committed MMO/global-scale claims  
**Classification:** Intentional bounds; conditional architecture expansion  
**Suggested owner:** Product / world and service architecture · **Dependencies:** Measured regional bottlenecks and explicit scale requirements

**What works.** Global coordinates, floating origin, bounded source streaming/LOD, regional paging and owner dormancy support one authoritative regional world with exact saved value.

**Open gap.** Limits include 512 total bodies, 128 principals, 4,096 regional pages/64 MiB checkpoint payload and one source level. Dormant owners still consume aggregate budgets. There is no distributed ownership/handoff, global adaptive source/clipmap system or unlimited indexed content store. Five-body crews make 128 principals different from 128 complete concurrent crews.

**Product impact.** More cold resource regions, global coordinates or larger caps do not produce an MMO or planet-scale populated simulation. Current limits may be appropriate for the selected regional product.

**Completion criteria.** First declare the target region, traversal, population and persistence envelope. Measure it. Only then design indexed/on-demand content, broader durable queries or regional ownership/handoff if required. Prove single ownership and no duplicate cargo/payment across failure, restart and handoff before increasing admission caps. Keep global data scaling distinct from population scaling.

Sources: [S21](#source-index), [S27](#source-index), [S09](#source-index), [S24](#source-index), [S06](#source-index).

## Conditional scope expansions

### Driving and trailers

Shared bicycle handling and support exist; dynamic suspension, tire/contact forces, rigid-body impacts, rollover and articulated trailer dynamics remain open. Decide fidelity first. Close through common local/server handling baselines, trailer reversing, slopes, low grip and impact cases on supported devices.

Sources: S20,S18,S31.

### Specialist jobs and deeper commercial play

Tow, medical and fire dispatches reuse transport stages; winches/physical towing, treatment and fire/hose mechanics are absent. Player employment, individual staff/rider schedules and fleet specialization remain open. Select careers and require a distinct start, equipment, actions, failure/recovery and durable reward; retain narrow labels otherwise.

Sources: S18,S31,S03.

### Competitive racing

Personal time trials exist. Complete events, synchronized starts/competitors, autocross authoring and ranked services are open. Promote only with an explicit competitive-play goal and authority/adjudication, duplicate-result, moderation and dispute acceptance.

Sources: S18.

### Historical, ecological and destructive simulation

Dated/political packs and bounded ecological gameplay exist. Evidence-backed historical Earth content, scientific populations/weather, hunting/taming, food webs, arbitrary source-building destruction and terrain excavation are not delivered. Each selected layer needs its own content, authority and evidence; these are not automatic regional-demo defects.

Sources: S26,S32,S12,S21.

### SDK publishing and broader authoring tools

The crates are intentionally delivered as source dependencies; published packages are optional productization. A universal quest/production editor, unrestricted glTF, humanoid retargeting and all animation graphs are not promised. Extend only for chosen developer workflows, with examples and compatibility policy.

Sources: S24,S08,S21.

## Suggested sequence

### 0. Correct the handoff

Applies to G01. Reconcile current protocol, Fleet navigation and version labels; preserve archives. Accept when a new integrator sees one current compatibility contract and accurate navigation.

### 1. Prove the regional player release

Applies to G02-G05, G07; G08 for claimed paths. Choose a Windows/hardware baseline and one career. Ship a distributable candidate; prove a viable strategy, first/returning-session guidance, safe travel/recovery and save/reopen on that executable. Include newcomer observation and named frame/memory targets.

### 2. Complete persistent content and community lifecycle

Applies to G06, G09-G12. Add the minimum community transitions, a representative production human/vehicle slice, a selected regional dataset and trusted delivery. Either support one tested persistent-world content upgrade or explicitly retain pinned worlds and old content.

### 3. Qualify a hosted service if selected

Applies to G13-G17. Complete identity/admin integration, WAN/load acceptance, measured storage pause budgets, recovery objectives and retained incident evidence. These are separate gates from the local player slice.

### 4. Expand only from a measured requirement

Applies to G18 and conditional backlog. Decide larger population/global traversal, advanced physics, specialist careers, competition and historical fidelity explicitly. Do not replace missing product acceptance with higher numeric caps.

## Source index

Paths in this table identify the material reviewed at the accepted 0.12 baseline. Unversioned documents may evolve in later releases; the original PDF and archived review/validation remain the audit record.

- **S01** — `docs/VALIDATION.md; docs/release-verification-v0.12.json`. Current accepted gates, native scope, compatibility and remaining qualification.
- **S02** — `docs/native-v12-verification.json`. Three final processes, 14 captures, exact named continuation checks and renderer/input environment.
- **S03** — `docs/COMPANIES.md`. First company; Cash accounting; Paired cash-basis career evidence; Verification and remaining scope.
- **S04** — `docs/verification/v0.12-final/career-report.json; purchased-career-report.json`. Measured careers and persisted accounting; the second filename shares the same directory.
- **S05** — `docs/verification/v0.12-final/traffic-recovery.json`. Exact saved-traffic fixtures and bounded conservation/continuation acceptance.
- **S06** — `docs/SCALING.md; docs/verification/v0.12-final/regional-sessions.json`. 0.12 combined prepared-terrain/cold-world workload, timings, memory and exclusions.
- **S07** — `docs/PERFORMANCE.md`. Measurement contract, 240-sample diagnostics, graceful-exit telemetry, timing interpretation and hardware guidance.
- **S08** — `docs/ASSET_PACKS.md`. Author and validate; Runtime behavior; Aggregate scene admission; Fixed import budgets.
- **S09** — `docs/SOURCE_BUNDLES.md`. Play and connect; Feature contract; Bounds and identity; Coverage/NoData; Admission/publication/updates.
- **S10** — `crates/terra-net/README.md; crates/terra-net/src/protocol.rs`. Current Protocol and authority paragraph says 7; actual constant is 9. Same README also declares terra-world/9.
- **S11** — `docs/GAME_FLOW.md`. Current flow, input boundaries and stale five-section Fleet list near the end.
- **S12** — `docs/GAMEPLAY.md`. First-session guide; Factions/shared property; Community ownership and irreversible public contributions.
- **S13** — `docs/SETTLEMENTS_AND_ECONOMY.md`. Current roles, funded work, economy policy and unarchived 0.9 introduction.
- **S14** — `docs/CLIENT_PREDICTION.md`. Bounded owner display replay, failure behavior and historical protocol-introduction wording.
- **S15** — `README.md`. Start here: source build, pinned toolchain, Linux prerequisites and unexercised Windows/macOS native builds.
- **S16** — `docs/PRODUCT_REVIEW.md; docs/REVIEW_GAPS_v0.12.md`. Implemented 0.12 corrections, exact native/automated evidence and remaining product/qualification boundaries.
- **S17** — `docs/GUIDANCE.md`. Player workflows; Acquisition and permissions; Verification and extension boundary.
- **S18** — `docs/MOTORTOWN_PARITY.md`. Terra capability/status rows and future acceptance gates. External benchmark claims were not re-researched.
- **S19** — `docs/CONTACTS_AND_ROUTING.md`. Supported walking shortcuts, source rules, junctions and bounded local recovery limitations.
- **S20** — `docs/VEHICLE_DYNAMICS.md`. Shared reference handling, autonomous continuation and remaining physical simulation work.
- **S21** — `docs/MMORPG_FOUNDATION.md`. Integration responsibilities; Current budgets; Concrete nonproduction boundaries; Practical expansion order.
- **S22** — `docs/GRAPHICS_AND_MAPS.md`. Rendering budgets, source geometry limits, actor replacement and game-owned visual integrations.
- **S23** — `docs/CONTROLS.md`. Keyboard/controller/RTS paths; Wheel calibration; Physical-device, atlas and accessibility limits.
- **S24** — `docs/INTEGRATION.md`. Source dependency model, provider/presentation contracts, source epoch, authority and production responsibilities.
- **S25** — `docs/DATA.md`. Included adapters; authored data boundaries; reproducible ingestion and remaining work.
- **S26** — `docs/HISTORY.md`. Included dated examples; source epoch/simulation clock; historical evidence and uncertainty.
- **S27** — `docs/REGIONAL_PERSISTENCE.md`. Authority barriers, validation scope, limits, recovery/migration and future regional ownership.
- **S28** — `docs/OPERATIONS.md`. Backup/recovery; TLS and capabilities; exact-principal controls; resume-only policy; real-session recovery.
- **S29** — `docs/MULTIPLAYER.md`. Current protocol, authoritative multiplayer behavior and embedding/deployment responsibilities.
- **S30** — `docs/NETWORK_REPLICATION.md`. Bounded queues/deltas/resync; TCP constraints; 0.9 delayed/fragmented network-condition evidence.
- **S31** — `docs/TRANSPORT.md`. Playable progression, vehicle/job coverage and remaining transport mechanics.
- **S32** — `docs/ECOLOGY.md; docs/LIVING_WORLD.md; docs/ARCHITECTURE.md`. Bounded authored ecology/economy, immutable source vs mutable overlay and regional architecture.
- **S33** — `CHANGELOG.md`. Version-introduction history and compatibility context; archives were treated as historical evidence.
