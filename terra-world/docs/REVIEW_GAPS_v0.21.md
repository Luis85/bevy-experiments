# Terra World 0.21 — improvement and complete gap report

Baseline: the supplied 0.20.0 source ZIP, reviewed on 29 October 2026. Three
dedicated specialists covered player/UX/accessibility, world/gameplay/rendering,
and reliability/storage/operations. The integrator covered networking,
architecture, QA, packaging and cross-module consistency. Implementations were
cross-reviewed before final acceptance. Historical reviews and receipts are
preserved without being counted as current execution.

The [product review](PRODUCT_REVIEW.md) explains findings and decisions. The
[implementation plan](IMPLEMENTATION_PLAN_v0.21.md) assigns work before dispatch.
[Validation](VALIDATION.md) records actual executed results and their limits.

## Selected closures and evidence boundaries

| Work | Implemented condition | Evidence boundary |
|---|---|---|
| WP01 Loading controls | Logical controller focus reaches actionable recovery controls; world input stays blocked and Saving stays noninteractive. | Actual action/system regressions; physical controllers remain separately qualified. |
| WP02 Modal editing | Native modal tab group, parent activation guards, closing-frame isolation, precise Apply/Discard target and wrapping. | Dispatched events with blocked and restored activation controls; native captures qualify only their shown states/dimensions. |
| WP03 Save comprehension | Four-mode copy describes managed JSON, direct JSON, regional and network persistence. | Copy tests checked against real save hooks; no new autosave promise. |
| WP04 Retail maintenance | Different assortments cannot panic; unsupported issuer prices skip before IDs, capacity or escrow. | Valid catalog fixture, supported requests, conservation and serialized continuity. |
| WP05 Geographic startup | Living/native readiness use real neighborhoods; only relevant resident/current-integrated keys count. | North/south six-tile, ordinary nine-tile, radius-zero, unrelated-page and invalid-neighborhood cases, with network/storage gates retained. |
| WP06 Local edit remeshing | Exact per-chunk feature context limits rebuilding while retaining source, LOD, quality and bounded retry rules. | One job of 49 pages for a local feature, two for shared fragments; no inferred GPU/frame-rate result. |
| WP07 Store admission | New native activations asynchronously validate committed state even with complete live state. | Real corruption before command/source mutation, valid live edits, one job per batch and multi-command progress. |
| WP08 Snapshot privacy | Unix publication temporaries start at 0600, including overwrite under permissive umask. | Isolated subprocess and failed-publication preservation; Windows ACLs unqualified. |
| WP08b First-launch continuity | Exclusive pre-launch record prevents missing required progress reopening an interrupted managed world as fresh. | Abrupt JSON/WAL exits, missing/corrupt/zero-generation refusal, concurrency, publication faults, legacy adoption and metadata-output protection. |
| WP09 Bounded acceptance | Gates/startup probes preserve partial output and fail on finite deadlines; package admission rejects timeouts. | Real process/descendant timeout and receipt/package regressions; Windows cleanup needs native proof. |
| WP10 Handoff | Current docs, full register, preserved history and exact deliverables. | Final receipt, source/binary/evidence hashes and archive-preservation report. |

“Scoped improvement” means the named condition has a specific implementation and
evidence. It does not close an entire product area. Final counts belong to
Validation; candidate runs and inspection are not accepted execution.

## Complete remaining-gap register

| ID / area | Current position | Remaining closure criteria / next proof |
|---|---|---|
| **G01 Documentation** | Scoped improvement: current review, dispatched plan, cross-reviews, validation and full register are connected. README history and the prior current review/validation remain preserved. | Maintain truthful current links, compatibility, evidence and delivered hashes; distinguish historical measurements from fresh execution. |
| **G02 Windows delivery** | **Open; P1 for the intended Windows PC.** Windows CI/packaging contracts remain. Linux evidence stays bounded to Linux. | Execute Windows gates and clean-machine create/play/save/reopen/recovery. Name supported OS/GPU/driver/runtime, installation and update policy. |
| **G03 Career progression** | Scoped improvement: valid heterogeneous merchants no longer crash maintenance. Existing finite careers, activity gates, costs and value-preserving rejection remain. | Observe beginner/advanced strategies, setbacks, living/labor/capital costs, treasury replenishment and long sessions without invented funding; set measured balance targets. |
| **G04 Continuing goals** | Scoped improvement: reachable Loading recovery, modal editing and accurate save copy. Six activities, owner commitments and reviewed restore consent remain. | Observe uncoached first/returning Supply, Home/Community and Road/Company journeys; measure misunderstood costs, stalls and recovery. Design/accept default RTS selected-owned-actor/needs visibility for all ownership contexts. |
| **G05 Navigation/recovery** | Physical, narrow-road and clustered-obstacle progress/contact matrices retained. No new geometry policy introduced. | Expand density, moving traffic, imported geometry and long interactions; require bounded progress or explicit value-preserving recovery. |
| **G06 Community lifecycle** | Departures, removals, invitations and explicit handover retained. | Decide unavailable-leader succession, dissolution, transfer/reclaim and treasury policy; verify online/offline authority and durable values. |
| **G07 Rendering qualification** | Scoped improvement: broad feature-edit invalidation closed with exact per-chunk context. Current integrated source identity, bounded two-worker scheduling, stale rejection and backoff retained. One early software-rendered capture showed partial/dark graphics before later settled frames; its cause was not determined. | Investigate first-visible-frame/shader warmup, and measure frame tails, residency and loading peaks on selected GPUs/scenes/settings; qualify production overrides. Arbitrary hung-thread preemption/containment remains open. |
| **G08 Accessibility/devices** | Scoped improvement: Loading logical controller paths, modal Tab focus, closing-frame ownership and session/editor wrapping. Native captures establish only shown software-rendered conditions. | Qualify physical device/reconnect journeys, screen readers and assistive access, contrast, motion comfort and localization with representative users. |
| **G09 Production content** | Original validated wildlife/vehicle/human examples, authoring tools and licenses preserved. | Extend skinning/retargeting, facial/equipment work, boarding/passengers/remote occupancy, audio and DCC presets; define shipping budgets and rights. |
| **G10 Regional data** | Scoped improvement: clipped/wrapped startup uses actual keys through population and native loading. Prepared-source/seam/coverage contracts retained. | Choose licensed shipping data and reproducible ingestion; resolve NoData, datum, topology, complex relations and bridge/underpass geometry with explicit fixtures. |
| **G11 Trusted delivery** | Scoped improvement: finite command/probe deadlines join exact 44-gate admission, hashed identities/logs and immutable receipts. This establishes consistency, not publisher trust. | Add signing/trust, release discovery, authenticated transfer/resume, installed-version selection and rollback; test target-platform installation/recovery. |
| **G12 Content migration** | Exact source/catalog identity and supported recovery retained; no migration introduced. | Design ID/geometry/catalog conflict policy with backup, rollback and authoritative validation; same-identity restore is not migration. |
| **G13 Public service controls** | Protocol-10 Welcome/source admission, TLS, private resume capabilities and Debug redaction retained. **Conditional P1 for Internet hosting.** | Establish enrollment/account recovery, authenticated operators, durable moderation/audit and abuse controls before public service claims. |
| **G14 WAN and capacity** | Real local TCP/TLS and finite regional-session workloads remain the correctness boundary. | Run representative delay/loss/reconnect and long soaks; derive supported populations, latency targets and resource limits from deployment measurements. |
| **G15 Storage stalls** | Scoped improvement: native activation cannot bypass committed validation; asynchronous bounded admission preserves live edits and per-batch progress. | Measure transition/checkpoint/maintenance tails and reduce stalls without weakening validation or durable acknowledgement. Final disk operations may still wait. |
| **G16 Operational recovery** | Scoped improvement: private JSON replacement, first-launch records and required-checkpoint admission protect managed continuity. Named recovery retains exact identities. | Close R4 sync uncertainty with complete failure semantics. Set retention/confidentiality/offsite owners and RPO/RTO; rehearse on target filesystems. Retain complete managed folders and external dependencies for archival copies. |
| **G17 Observability** | Scoped improvement: timeouts retain hashed logs and failure classification; diagnostics recheck protected metadata paths on every publication. Existing opt-in bounded history/privacy retained. | Add deployment retention, crash diagnostics, external audit/alerts and ownership; demonstrate detection. Stale/absent captures never establish health. |
| **G18 Global architecture** | Regional single authority remains explicit; no distributed economy or global seamless authority claim added. | Establish population/data/service requirements before partitioning, cross-shard ownership/economy and adaptive delivery. |

## Prioritized continuation

1. **Windows proof (G02):** exact acceptance and the ordinary desktop journey on
   the intended PC, including interrupted launch/recovery and retained settings.
   Publish its platform receipt and runtime dependencies.
2. **Observed journeys (G03/G04/G08):** uncoached new/returning play, physical
   controller text/recovery paths and assisted access. Record specific stalls,
   misunderstandings and unreachable actions before expanding the interface.
3. **Selected actor clarity (UX04):** a compact owned-actor/needs summary with
   explicit empty, multi-selection, vehicle and remote-partial states. Accept at
   1100×700 and 150% text before shipping the additional layout.
4. **Directory publication uncertainty (R4/G16):** distinguish failure before
   publication from uncertainty after rename in existing publishers. Inject file
   and parent-sync faults, retain recoverable bytes and never promise rollback
   after publication is visible.
5. **Runtime budgets (G07/G15):** physical GPU loading/frame tails/residency and
   storage transition tails on representative content/hardware. Work-count tests
   stay correctness guards while performance is measured separately.

Community succession/dissolution/treasury policy, sustainable finite careers,
production art/audio, licensed geography and content migration require explicit
design and acceptance fixtures. Trusted updates, public accounts/moderation,
WAN capacity and offsite recovery need an intended environment and owners.
Global partitioning follows those measured requirements.

## Limits of the launch record

- It records initialization admission, not a successful save, healthy disk,
  successful session or complete backup.
- An interrupted first launch without a checkpoint fails closed. Recovery or
  an explicitly separate new world is intentional; automatic reset would erase
  the distinction the record preserves.
- Already-lost progress behind legacy `checkpoint_present=false` with no launch
  record cannot be identified retroactively. Valid legacy progress gains a record
  on admitted launch.
- Older launchers do not enforce this protocol. Arbitrary external filesystem
  replacement, power loss, Windows ACLs and concurrent resumed sessions are not
  newly qualified guarantees. Use verified recovery for path-bound relocation.

## Evidence and preservation

Original assets, data, authoring inputs and historical reports remain in the
source archive. Review notes are in `verification/v0.21-review`; candidates are
in `verification/v0.21-preflight`; final acceptance/native evidence have their
own directories. Save 1, regional 1, protocol 10 / `terra-world/10` and asset 1
remain targets. Validation identifies exact accepted source/binary/platform.
