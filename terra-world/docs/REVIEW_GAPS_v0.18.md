# Terra World 0.18 — improvement and open-gap report

Review date: **29 October 2026**. Baseline: the attached **terra-world-0.17.0.zip**.
This is a bounded reliability and playtest-foundation increment. It does not
close every area in the G01–G18 product register.

## Implemented changes

| Area | Reproduced defect | Result |
|---|---|---|
| Storage/recovery | Poisoned store mutex could be returned as a retryable completion with default status. | Poison is terminal, preserves the last acknowledged checkpoint and requires validated reopen. Read-only adapters expose the same unavailable state. |
| Economy | Owner-paid assigned-vehicle services could add a company expense before a wallet/treasury rejection. | Prepare receipt, perform checked payment, then commit receipt. Rejected fuel, repair, paint, both upgrades and renewal preserve the complete serialized state. |
| Player input | Standard pads could resume held world input after focus/device changes. | Neutral rearm blocks world intent, retaining menu and independent keyboard access. Zero trigger neutrality matches raw-throttle consumption. |
| Delivery | Linux-safe archive names could alias or fail on Windows; raw duplicate evidence identities could be hidden by dictionary insertion. | Validate portable names and full file/directory namespace, including raw evidence names, before publication. |

The [implementation plan](IMPLEMENTATION_PLAN_v0.18.md) preceded dispatch of
fresh dedicated agents for reliability, gameplay, player experience and delivery.
A fifth agent reviewed their work independently across product perspectives;
a sixth owns fresh Linux session observations. Integration corrections include
read-only poison visibility, exact-zero trigger rearm and raw evidence-name checks.

## Final automated acceptance

**44 gates passed**, with **933 distinct Rust tests** across **1,019 passing
executions**, zero failed and zero ignored. The separate Python packaging suite
passed **11 tests** with zero skips on Linux. All **368 implementation inputs**
and captured reports/logs remained unchanged. The accepted native executable is
`sha256:e16c25fabf30d8e96ff6b68cea99766ea1f4cc4d67afc1fbaeaff4433ffa8881`.
See the [canonical receipt](verification/v0.18-acceptance/gates.json).

Fresh Linux observation completed menu creation → play → Save and return →
named reopen → Save and quit with exit **0**. Committed generation advanced
**34 → 52** while the compared identity/character/wallet/inventory fields matched.
Controls help and actual paused layouts were inspected at the sizes named in
[the native review](verification/v0.18-native/native-review.md). The session began
before gate completion; the final accepted executable hash matches it. This is
neither Windows nor physical-device or uncoached-play qualification.

## Review and polishing

The [independent review](verification/v0.18-review/independent-review.md)
records player goals, design/UX, accessibility/input, economy/ownership,
simulation/navigation, community lifecycle, content/graphics/data, reliability,
privacy/operations and developer/delivery findings. Specialist evidence:
[reliability](verification/v0.18-review/reliability.md),
[gameplay](verification/v0.18-review/gameplay.md),
[player experience](verification/v0.18-review/player-experience.md) and
[delivery](verification/v0.18-review/delivery.md).

The incumbent visual design and all original assets/geographic fixtures are
preserved. No new pricing, progression, account authority, external telemetry,
public service controls or content migration is introduced. Save **1**, regional
schema **1**, wire **10**, ALPN **terra-world/10** and asset schema **1** remain.

## Validation and delivery

[Current validation](VALIDATION.md) is the authority for newly executed checks,
exact totals, native observations and their limits. The 0.17 review/validation
are archived unchanged as [Product review 0.17](PRODUCT_REVIEW_v0.17.md) and
[Validation 0.17](VALIDATION_v0.17.md). Earlier receipts remain historical.
Source-hash checks bind the accepted executable to its inputs; archive inspection
checks preservation and byte integrity. Hashes do not authenticate a publisher.

## Complete remaining-gap register

“Scoped improvement” means a concrete condition is implemented and checked
within its declared boundary. It does not close the whole product area.

| ID / area | Current position | Remaining closure criteria |
|---|---|---|
| **G01 Documentation** | Scoped improvement: current implementation plan, specialist and independent review, fresh validation and preservation evidence are connected. | Maintain current links, compatibility, delivery hashes and truthful claims each release; preserve historical measurements. |
| **G02 Windows delivery** | Open; **P1 for the user's Windows PC**. | Execute Windows gates and a clean-machine create/play/save/reopen/recovery journey; name supported OS/GPU/runtime and delivery policy. The Linux ZIP cannot satisfy this. |
| **G03 Career progression** | Scoped improvement: rejected assigned-vehicle services preserve complete state, including company expense receipts; previous careers remain regression cases. | Test beginner/advanced strategies, setbacks, living/labor/capital costs, treasury replenishment and longer sessions without invented funding. |
| **G04 Continuing goals** | Six explicit goals and owner commitments retained; contextual driving handoff repaired; playtest protocol supplied. | Observe uncoached first/returning Supply, Home/Community and Road/Company journeys and measure misunderstood costs, stalls and recovery. |
| **G05 Navigation/recovery** | Existing physical, narrow-road and clustered-obstacle regression matrices remain. | Expand declared geometry, density and long interactions; require bounded progress or explicit value-preserving recovery. |
| **G06 Community lifecycle** | Existing departures, removals and handover retained. | Define unavailable-leader succession, dissolution, transfer/reclaim and treasury policy; verify offline permission and durable values. |
| **G07 Rendering qualification** | Bounded model/loading and timing evidence is easier to retain. | Measure frame tails, real residency and loading peaks on selected GPUs; production overrides require equivalent budgets. |
| **G08 Accessibility/devices** | Scoped improvement: standard-pad focus/device handoffs require neutral world input while retaining menus and keyboard; synthetic regressions added. | Qualify full physical-device journeys, reconnect ordering, assistive access, contrast, motion comfort and localization with representative users. |
| **G09 Production content** | Existing validated, original wildlife/vehicle/human examples preserved. | Extend skinning/retargeting, facial/equipment work, boarding, passengers/remote occupancy, audio, DCC presets and shipping budgets/licensing. |
| **G10 Regional data** | Connected prepared input/seam/coverage contracts retained. | Choose licensed shipping data and reproducible ingestion; resolve NoData, datum, topology, complex relations and bridge/underpass geometry. |
| **G11 Trusted delivery** | Scoped improvement: portable archive namespaces reject Windows aliases, reserved devices and evidence shadows; local integrity evidence retained. | Add publisher trust/signing, release discovery, authenticated transfer/resume, installed-version selection and rollback. Hashes alone do not authenticate a publisher. |
| **G12 Content migration** | Exact recovery and source identity remain; no migration introduced. | Design explicit ID/geometry/catalog conflict migrations with backup, rollback and authoritative validation. |
| **G13 Public service controls** | Open; conditional **P1 for Internet hosting**. | Establish enrollment/recovery, authenticated operators, durable moderation/audit and abuse controls. |
| **G14 WAN and capacity** | Finite loopback/session workloads remain the acceptance boundary. | Run representative delay/loss and long soaks; derive supported populations and latency targets from measured results. |
| **G15 Storage stalls** | Scoped improvement: poisoned store locks also require validated reopen; last acknowledged status survives and uncertain commits remain blocked. | Measure transition/maintenance latency tails and reduce stalls without weakening complete validation or durable acknowledgements. Final disk operations can still wait. |
| **G16 Operational recovery** | Scoped improvement: actionable terminal close/reopen joins named backup/recovery and exact committed-state checks. | Set retention, confidentiality, offsite copies, owners and RPO/RTO; rehearse the intended deployment and extend unsupported source/catalog policy explicitly. |
| **G17 Observability** | Scoped improvement: menu opt-in, bounded local history, status/age, session phase and single-writer failure handling. | Add deployment retention, crash diagnostics, external audit/alerts and ownership; validate detection. Stale/absent captures never establish health. |
| **G18 Global architecture** | Conditional expansion; regional single authority remains explicit. | Establish actual population/data requirements before authority partitions, cross-shard ownership/economy and adaptive delivery. |


## Next acceptance priorities

1. **Windows on the intended PC:** pinned-toolchain gates, real PE package,
   clean extraction and create/play/save/reopen/recovery; record OS/GPU/input.
2. **Uncoached playtests:** first and returning Supply, Home/Community and
   Road/Company sessions; collect recurring stalls, misunderstood costs and
   whether players can follow the recovery guidance.
3. **Measured hardware and content:** frame/loading tails, real controllers and
   wheels, production asset residency, geography and navigation density.
4. **If Internet hosting is intended:** accounts/moderation, WAN/soak capacity,
   operational retention, backup owners and rehearsed recovery objectives.

These are remaining acceptance tasks, not work established by synthetic input,
loopback workloads, a Linux software renderer or correctness regressions.
