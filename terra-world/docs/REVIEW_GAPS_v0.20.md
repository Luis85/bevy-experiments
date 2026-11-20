# Terra World 0.20 — improvement and open-gap report

Baseline: saved0.19 source ZIP, reviewed29 October 2026. Six fresh dedicated
specialists reviewed player goals/UX/accessibility, gameplay/economy/progression,
world/rendering/performance/content/data, multiplayer/security/architecture,
storage/recovery/operations, and QA/delivery/developer handoff. Each reviewed
before implementation, with peer cross-review before final acceptance.

## Selected gap closures

| Area | Selected improvement | Acceptance |
|---|---|---|
| Player safety | Coordinated F9/Load consequence review bound to exact current world and saved target | Cancel/stale/confirmed routes, both entrypoints and queue-time recheck |
| Presentation recovery | Persistent dirty mesh work, bounded worker failure retry/backoff and diagnostics | Deterministic disconnect/poison/retry/source replacement/budget cases |
| Network admission | Shared Welcome checks, requested-manifest continuity and reject raw snapshot bypass | Malicious real loopback peers plus valid protocol10 flows |
| Gameplay | Expired unclaimed supply offers cannot be accepted by stale ID before maintenance | Deadline boundary, complete-state rejection and legitimate refreshed acceptance |
| Storage integrity | Fully materialized hydration validates committed schema/payload | Same-generation corruption/schema mutation, unchanged cache reuse and state preservation |
| Release evidence | Immutable terminal receipt published exclusively and read back | Fault-injection/publication tests; package only completed-gates.json |

The [implementation plan](IMPLEMENTATION_PLAN_v0.20.md) records scope/owners.
Specialist and cross-review notes are under `verification/v0.20-review`.
[Validation](VALIDATION.md) records only newly executed checks. Existing0.19
receipt reconstruction is historical; this increment does not recertify it.
Save1, regional1, wire10/terra-world/10 and asset1 remain compatibility targets.

<!-- final020:start -->
## Final executed acceptance and review corrections

**44 gates passed · 961 distinct Rust tests · 1050 passing Rust
executions · 28 separate Python delivery tests (0 skips).**
All **370 frozen implementation inputs**, logs and reports match their
accepted identities. The direct runner-written `completed-gates.json` matches
the progress alias; **no 0.20 reconstruction** was needed. Exact records and scope
are in [Validation](VALIDATION.md). Accepted binary:
`sha256:f299f9af1bc2b3669a50de620387a41cd4aa3e9e2bc90648fd2705e7bcbd02f6`.

Six dedicated reviews covered player/UX/accessibility, gameplay/economy,
world/content/performance, multiplayer/security/architecture, storage/recovery,
and QA/delivery. Cross-review corrected stale queued restore consent through a
unique captured review token and bound packaging to the actual completed receipt
so failed publication cannot be hidden by a passed alias. Source inspections and
executed regressions remain separate evidence. Native windowed, Windows,
physical/assistive-device and GPU acceptance were not established by these tests.

<!-- final020:end -->

## Complete remaining-gap register

“Scoped improvement” means a concrete condition is implemented and checked
within its declared boundary. It does not close the whole product area.

| ID / area | Current position | Remaining closure criteria |
|---|---|---|
| **G01 Documentation** | Scoped improvement: six fresh specialist reviews, cross-reviews, a pre-dispatch plan, current validation and preservation evidence are connected. | Maintain current links, compatibility, delivery hashes and truthful claims each release; preserve historical measurements. |
| **G02 Windows delivery** | Open; **P1 for the user's Windows PC**. | Execute Windows gates and a clean-machine create/play/save/reopen/recovery journey; name supported OS/GPU/runtime and delivery policy. The Linux ZIP cannot satisfy this. |
| **G03 Career progression** | Scoped improvement: active-session gates cover direct settlement commands and controlled motion; rejected commands preserve complete state and explicit resume restores play. Prior finite careers remain regression cases. | Test beginner/advanced strategies, setbacks, living/labor/capital costs, treasury replenishment and longer sessions without invented funding. |
| **G04 Continuing goals** | Six explicit goals and owner commitments retained; long launcher statuses scroll and paths wrap without replacing reviewed actions. 0.20 adds coordinated F9/Load consequence review with context-bound target and confirmation identity. | Observe uncoached first/returning Supply, Home/Community and Road/Company journeys and measure misunderstood costs, stalls and recovery. |
| **G05 Navigation/recovery** | Existing physical, narrow-road and clustered-obstacle regression matrices remain. | Expand declared geometry, density and long interactions; require bounded progress or explicit value-preserving recovery. |
| **G06 Community lifecycle** | Existing departures, removals and handover retained. | Define unavailable-leader succession, dissolution, transfer/reclaim and treasury policy; verify offline permission and durable values. |
| **G07 Rendering qualification** | Scoped improvement: stale mesh completions cannot integrate against replacement source chunks; current source retries stay bounded. 0.20 closes bounded disconnect/poison retry (WP02) with backoff and visible telemetry; hung-thread preemption and broad edit invalidation (WP03) remain open. | Measure frame tails, real residency and loading peaks on selected GPUs; production overrides require equivalent budgets. |
| **G08 Accessibility/devices** | Scoped improvement: launcher status structure preserves fixed footer actions and character-level wrapping; incumbent neutral gamepad rearm remains. Structural tests do not prove actual native/assistive-device reachability. | Qualify full physical-device journeys, reconnect ordering, assistive access, contrast, motion comfort and localization with representative users. |
| **G09 Production content** | Existing validated, original wildlife/vehicle/human examples preserved. | Extend skinning/retargeting, facial/equipment work, boarding, passengers/remote occupancy, audio, DCC presets and shipping budgets/licensing. |
| **G10 Regional data** | Connected prepared input/seam/coverage contracts retained. | Choose licensed shipping data and reproducible ingestion; resolve NoData, datum, topology, complex relations and bridge/underpass geometry. |
| **G11 Trusted delivery** | Scoped improvement: desktop admission requires the full canonical gate roster, hashed distinct test identities and consistent integer test counters, retaining source/log/binary/platform checks. 0.20 adds exclusive read-back-verified terminal receipts and requires matching completion evidence for publication. This is structural consistency, not publisher trust. | Add publisher trust/signing, release discovery, authenticated transfer/resume, installed-version selection and rollback. Hashes alone do not authenticate a publisher. |
| **G12 Content migration** | Exact recovery and source identity remain; no migration introduced. | Design explicit ID/geometry/catalog conflict migrations with backup, rollback and authoritative validation. |
| **G13 Public service controls** | Scoped privacy improvement: protocol Debug redacts bearer resume capabilities without changing wire JSON. Public-service controls remain open; conditional **P1 for Internet hosting**. 0.20 adds shared Welcome/requested-source admission and rejects raw snapshot bypass (NET-02); public enrollment/moderation remains separate. | Establish enrollment/recovery, authenticated operators, durable moderation/audit and abuse controls. |
| **G14 WAN and capacity** | Finite loopback/session workloads remain the acceptance boundary. | Run representative delay/loss and long soaks; derive supported populations and latency targets from measured results. |
| **G15 Storage stalls** | Scoped improvement: schema-1 admission checks structural columns, keys, indexes and foreign-key policy; duplicate regional identities reject instead of overwriting. 0.20 validates committed state even on initialized/full hydration shortcuts, retaining valid cache reuse and live edits. Incumbent poisoned-worker reopen semantics remain. | Measure transition/maintenance latency tails and reduce stalls without weakening complete validation or durable acknowledgements. Final disk operations can still wait. |
| **G16 Operational recovery** | Scoped improvement: strict storage integrity admission joins named backup/recovery, exact committed-state checks and supported existing-store continuity. | Set retention, confidentiality, offsite copies, owners and RPO/RTO; rehearse the intended deployment and extend unsupported source/catalog policy explicitly. |
| **G17 Observability** | Scoped improvement: typed Hello/Welcome Debug redacts capabilities; local diagnostic opt-in/history/age/single-writer failure handling remain. Explicit wire serialization still contains private capabilities. | Add deployment retention, crash diagnostics, external audit/alerts and ownership; validate detection. Stale/absent captures never establish health. |
| **G18 Global architecture** | Conditional expansion; regional single authority remains explicit. | Establish actual population/data requirements before authority partitions, cross-shard ownership/economy and adaptive delivery. |


## Remaining acceptance priorities

Windows execution on the intended PC; uncoached first/returning and sustained
finite-economy journeys; unavailable-leader/community policy; dense navigation;
actual GPU/loading/residency and physical/assistive-device measurements;
production art/audio and licensed geography; explicit content migration;
authenticated signed updates/rollback; public enrollment/moderation and WAN
soaks; storage tails, offsite retention and operational RPO/RTO; requirements-led
global authority partitioning. The inherited G01–G18 criteria above remain open
unless a specific0.20 closure and its evidence is stated. Correctness tests,
loopback workloads and compiled Linux code cannot qualify those environments.

Native windowed review was blocked before game launch in0.19 by Unix-socket
policy. No repeat/bypass is attempted; current UI tests establish only their
stated entity-tree/action/authority behavior, not rendered pixels or hardware.

## SDK and consent handoff

The local native UI uses the new ReviewedRestore/LoadReviewed path; existing
explicit trusted-host SandboxIntent::Load remains available. Rust callers with
exhaustive matches over SandboxIntent must handle the added local variant.
JSON consent checks the bounded exact bytes parsed; regional consent checks the
reviewed generation inside validated restore before installing state/cursors.
It does not authenticate malicious same-generation payload substitution as a
new security property. Save/wire/content versions are unchanged.
