# Terra World 0.22 — comprehensive product review

Review began **29 October 2026**, against the delivered **0.21.0** source.
The user requested an all-perspective review, primary-source research, a
subagent-driven implementation plan and completed next framework increment.
The [plan](IMPLEMENTATION_PLAN_v0.22.md) was written before dispatch; the
[research synthesis](RESEARCH_v0.22.md) connects guidance to decisions.
[Validation](VALIDATION.md) is the authority for what actually executed.

## Product assessment

Terra has a substantial reusable foundation: georeferenced streaming, source
provenance, ownership, RTS and possession, finite commerce, transport, community
progression, regional persistence and a native reference game share domain
rules. Existing reviews already identify the larger production and deployment
boundaries. The next increment should make the ordinary play loop clearer and
more dependable, while closing a real lifecycle dead end.

The highest-value findings were **inaccurate or missing selected-actor context**,
**no exit from an unused solo community**, **ambiguous file-publication outcomes**
and **no lifetime writer ownership for resumed managed worlds**. They affect
comprehension, consent or progress safety. Source-only architectural expansion,
additional currencies or speculative distributed systems would not resolve them.

The selected 0.22 pass addresses those findings, one native/host ecology-policy
difference, and independent desktop inspection. It keeps the original visual
identity, finite economy, current source identities and recorded historical
work. It does not turn Linux software-rendered evidence into Windows, physical
GPU or public MMORPG qualification.

## Review method and independent ownership

| Review owner | Coverage | Evidence |
|---|---|---|
| Player specialist | Entry and return journeys, actor/need/action context, accessibility, focus, text, controls and mental-model consistency | [Player findings](verification/v0.22-review/player-experience.md), current native captures, resolver and actual dispatched-system regressions |
| World/framework specialist | Community lifecycle, conservation, cold ownership, economy, transport, ecology, navigation, rendering boundaries and SDK extension seams | [World findings](verification/v0.22-review/world-framework.md), domain/adapter/TLS fixtures |
| Reliability specialist | Save/recovery stages, privacy, concurrency, session lifetime, platform contracts and operational delivery | [Reliability findings](verification/v0.22-review/reliability-delivery.md), file/process/marker faults and admission tests |
| Integrator | Product scope, transport/identity boundaries, research synthesis, cross-module review, versioning, release tooling, native journeys and preservation | This review, package verifier, current validation and final source/archive records |

Reviewers rotated across another owner's implementation. The player specialist
reviewed community consent and layout; the world specialist reviewed storage and
recovery conservation; the reliability specialist reviewed desktop inspection.
The integrator inspected action targets, lifetime ownership, compatibility and
all evidence. Negative hypotheses were retained honestly: for example, apparent
transport intermediate writes already occur inside a validated candidate-state
transaction and were not treated as a confirmed partial-write defect.

## Assessment across product perspectives

| Perspective | Strength and concrete finding | 0.22 disposition / remaining proof |
|---|---|---|
| Product fit and scope | A native reference game already exercises reusable pure-domain and adapter seams. Broad ambition spans real geography, simulation and online worlds. | Prioritize complete player/persistence outcomes. Keep G01–G18 requirements explicit before production/distribution scaling. |
| New-player onboarding | Native creation/review/loading, live Activities guidance and written source distinction exist. Default Activities concealed actor needs. | Shared HUD actor context closes the local information gap; uncoached first-action and comprehension evidence remains open. |
| Returning-player continuity | Commitments and recovery are live-state interpretations, but ambiguous persistence and concurrent sessions threaten confidence. | Explicit publication outcomes plus lifetime managed lease; retain observed returning-career journeys as G04 proof. |
| Interaction and consent | Quoted financial actions and reviewed restore already bind context. Generic queued community Confirm could be applied to a later review. | Native displayed actor/review identity is retained and rechecked. Stale clicks do not silently consume a replacement review. |
| Accessibility and layout | Adjustable text, logical controller paths, scrolling/focus and named needs exist. Long single tokens and group semantics need attention. | Word-or-character wrapping, written ownership/scope and consistent labels/bars. Native minimum-size validation is bounded; broad typography/narration/device qualification remains. |
| Gameplay progression | Finite supplies, construction, nature, territory and transport form a connected loop. Experimental solo factions permanently consumed affiliation capacity. | Safe unused-community dissolution frees the choice without refunds or asset reassignment. Long-career balance still needs measured play. |
| Economy and value integrity | Escrow, stock/capacity, quoted totals and atomic candidate transactions are present. | Preserve all ledgers and no-mutation rejection. New dissolution moves no cash, stock or property. No invented reward/treasury source. |
| Community governance | Explicit invitations, membership and leadership transfer exist. Asset-owning dissolution/succession needs policy. | Only sole-leader, no-shared-assets dissolution; far/cold ownership blocks. Populated governance and treasury policy remain G06. |
| Living simulation and ecology | Fixed authority and bounded work are established. Native observers included dormant owners unlike the host policy. | Exclude dormant-owned observers with actual adapter regression; retain rates, positions and cold-region semantics. |
| Navigation and vehicles | Shared handling and authoritative contacts have bounded geometry fixtures. | Preserve them. Dense traffic, imported geometry, inaccessible seeded services and sustained autonomous progress need dedicated workloads. |
| Geography and data truth | Source identity, attributed presets and synthetic labels exist. Real GPS does not establish observed population/business activity. | Retain explicit provenance and all original data. Datum, NoData, topology, licensed shipping datasets and constrained-source placement remain G10. |
| Rendering and performance | 0.21 localized remeshing and retained/fallback presentation reduce unnecessary work. Physical budgets remain workload-specific. | Keep unchanged correctness rules; inspect real native pixels for affected UI. Do not infer FPS or first-frame cause from CPU mesh integration. |
| Framework/DX and API | Nine crates separate domain, providers, persistence, transport and native integration. | Add small read-only and publication seams, document public enum/error additions, and deliberately advance wire compatibility. No unnecessary dependency upgrade. |
| Multiplayer and authority | TLS/ALPN, bounded framing, private capabilities, ownership and replay checks are present. | Run new community lifecycle over actual TLS/restart. Native review binding remains distinct from host active-body command semantics. Public accounts and WAN capacity stay open. |
| Security and privacy | Private native snapshots and guarded outputs exist, but duplicated publishers differed and new lock metadata needs protection. | Align private publication, retain token redaction, protect lease/profile/witness aliases. Cooperative filesystem assumptions and platform limits are explicit. |
| Reliability and recovery | SQLite snapshot/generation/source validation is strong. Ignored parent sync and generic cleanup obscured whether output already existed. | Structured publication stages, completed-tree flushing, retained published artifacts and actual failure-path tests. Physical power-loss proof remains separate. |
| Concurrency | Regional CAS detects later stale commits; resumed managed JSON had last-exit-wins exposure. | Stable nonblocking OS lease before initialization, held through final metadata. Existing unlocked lock files do not imply a running session. |
| Delivery and support | Canonical gates bind source/log/report/binary identities; Linux/Windows workflows exist. Consumers lacked a standalone extracted inspection path. | Independent bounded package verifier and explicit startup smoke. Windows execution, publisher signing, upgrades and rollback remain distinct gates. |
| Observability and operations | Diagnostics are opt-in and bounded; release command deadlines retain failure evidence. | Publication errors and inspection reports describe their scope. Deployment alerting, offsite retention and RPO/RTO still require an environment and owner. |

## Implemented design decisions

### 1. Actor context belongs beside the current task

The shared read-only actor model distinguishes no selection, a single owned body,
inspect-only bodies, groups, vehicles, unavailable identities and received remote
scope. It does not fill missing data with the current avatar. Group needs are
explicit minima across selected owned biological bodies, excluding vehicles and
foreign bodies. A driver appears only for the exact admitted active owned
vehicle and valid avatar association.

The existing footer carries compact context without adding a new panel or
changing the shared world-hit boundary. Command shows complete context and
consistent need bars. Long names wrap in inspection and are bounded in compact
text. Native Eat/Drink activation retains displayed player/restore/body identity,
checks current context and queued body transitions, then submits the existing
domain operation. Stop/Rest snapshots owned selected IDs. Vehicle Rest means
stopping the order, not restoring a driver's biological needs.

The remote domain Consume contract is still an authoritative active-body
operation; no new cross-session reviewed-target wire contract is claimed.
The specific native stale-action protections and their tests are documented in
[Controls](CONTROLS.md) and [Game flow](GAME_FLOW.md).

### 2. Close an unused community without inventing public-asset policy

`DissolveFaction` permits only the active sole member and current Leader of a
faction without shared plots or settlement territories. Outstanding invitations
are explicitly revoked. The faction's own record is removed; personal funds,
goods, property, commitments and other groups are unchanged. IDs are not reused.

All plots and territories are global in validated regional authority, so cold
shared houses/farms remain protected through their ownership invariants. A
spatial client may show visible blockers but cannot prove absent distant assets;
the host checks current complete authority. The review names the faction and
consequences, offers Cancel, and binds confirmation to the displayed review
instance. New shared property, membership, selection or restore context requires a
fresh decision or an authoritative refusal.

### 3. Report the stage of persistence failure honestly

The shared private publisher distinguishes a destination that was not replaced
from newly visible bytes whose synchronization could not be confirmed. A
post-publication failure retains the final bytes, never promises rollback and
never becomes a false successful save acknowledgement. Replace and no-clobber
intent, private staging, file sync, relative-parent handling and checked Unix
namespace synchronization are explicit.

Recovery additionally separates each completion marker. A complete backend
bundle may be retained even if its native wrapper fails; the missing wrapper is
not reported complete. Final-marker uncertainty must not trigger recursive
cleanup of an already visible validated artifact. Newly created ancestors are
synchronized before payload publication, and completed payload trees are flushed
before completion markers. Host snapshots, private session identities, domain/
native saves, launcher metadata and telemetry use the relevant shared boundary.

### 4. Own a managed session until its final write

A current managed runtime acquires a stable per-profile OS lease before progress
initialization and retains the file handle through final metadata persistence.
Contention refuses admission before progress changes. Handle closure releases
ownership after a clean or abrupt process exit; the lock file is not deleted.
The lease is separate from the 0.21 launch witness and the regional generation
comparison. Each covers a different state boundary.

The mechanism is cooperative. Older executables, direct SDK writers, arbitrary
external filesystem replacement and network filesystems do not acquire a new
universal safety guarantee. Private Unix file modes and Linux execution do not
qualify Windows ACLs or device-level crash durability.

### 5. Inspect the extracted handoff independently

The included Python verifier needs no source checkout or Rust toolchain. Passive
mode checks exact bounded inventory, portable names, links/aliases, hashes,
architecture and terminal-receipt association. Optional `--smoke-help` explicitly
runs the verified executable on a matching host with a finite deadline. Reports
are written outside the package by the caller so the clean inventory remains
meaningful. Added caches/mods are not silently deleted to force a pass.

This establishes consistency, not trusted publisher identity or independent
execution of the recorded test suite. It helps separate missing/corrupt content
from basic runtime startup, while physical Windows/GPU/device qualification
remains a separate acceptance step.

## Cross-review corrections

| Finding | Correction and reason |
|---|---|
| Generic queued Confirm could consume a newly opened community review | Process-local checked review token captured from the displayed review, retained in the queued confirmation and compared at dispatch. Old confirmation cannot consume new consent. |
| Eager caller directory creation hid new ancestors from the publisher | Shared checked creation records/synchronizes newly created entries; callers use it before staging. An immediate file-parent sync alone does not establish ancestor durability. |
| Completed inner recovery could be confused with completed native wrapper | Track the relevant final marker structurally; preserve completed inner output, report missing/uncertain wrapper honestly, never delete useful published output on later failure. |
| Extracted inspection accepted hard links and read one receipt alias without its JSON bound | Reject hard-linked regular payloads and use the bounded reader for both terminal/alias records. Counter types are strict, avoiding boolean/float success aliases. |
| Need availability and action dispatch used different notions of selection during possession | Shared owned-selection iteration governs group actions; driver/vehicle explanations state their actual effects. |
| Actual native inspection showed an inspect-only resident alongside a generic F2 possession hint | Selection feedback now uses the current principal and actual primary actor, retaining owned movement guidance without advertising unavailable possession. Command targets are unchanged. |
| Actual F6 entry highlighted Confirm despite the review's Cancel-first layout | The management navigation system now enters a new visible review on Cancel and waits for that control if necessary; it cannot fall back to Confirm. An actual F6 capture/navigation/activation regression protects the handoff. |

Independent reports are [community/UX](verification/v0.22-review/world-cross-review.md),
[storage/recovery](verification/v0.22-review/reliability-cross-review.md) and
[delivery](verification/v0.22-review/delivery-cross-review.md). Compilation/test
fixture corrections are retained separately from product defects in Validation.

## Native flow evidence

Fresh baseline screenshots in `verification/v0.22-native` were captured from the
accepted 0.21 binary during this review. RTS Activities showed no actor needs;
crew selection showed multiple selection rings while the footer still named one
explorer. Pause/settings/save-and-quit were exercised with an isolated local
world. The start source identity matched 0.21 acceptance; the process continued
while 0.22 source edits began, so its completion receipt correctly records source
change and is not final 0.22 acceptance.

The [native record](verification/v0.22-native/README.md) now contains the actual
managed create/play/community/save/reopen journey at 1440×900 and 1100×700 with
150% text. Candidate inspection confirmed the actor/group/empty/inspect-only,
first/third-person and vehicle/driver states, then found the ownership-toast and
Confirm-default focus defects. One correction batch was rebuilt and confirmed.
The final F6 entry visibly selects Cancel with the complete named consequences;
Enter cancels, and a later review requires deliberate navigation to Confirm.

The direct final process exited zero with 378 unchanged implementation inputs.
Closed-world inspection returned SQLite integrity `ok`, generation 60, unchanged
wallet/inventory/witness/lease marker, no factions and next domain ID 3 after
creating/dissolving IDs 1 and 2. The record gives the exact source/binary
association; canonical release acceptance remains separately recorded in
Validation. Each cited image was inspected. Baseline/candidate/confirmation
phases remain separate. The native walkthrough uses developer operation and a
software graphics adapter; it does
not establish an uncoached user study or physical-device test.

## Compatibility, preservation and remaining priorities

Release **0.22.0**, wire **11**, TLS **terra-world/11**; upgrade clients and hosts
together. Domain save **1**, regional schema **1**, asset schema **1**, minimum
Rust **1.97.1** and pinned acceptance **1.99.1** remain. New public command/error
variants can require downstream exhaustive matches to be updated. No automatic
world-source, catalog or asset migration is introduced.

The prior current review and validation are preserved byte-for-byte as
`PRODUCT_REVIEW_v0.21.md` and `VALIDATION_v0.21.md`; the original source delivery
manifest is retained under the current review evidence. Original assets, data,
configurations, authoring files and historical records remain. Final source
packaging repeats the original-archive comparison and records every allowed edit.

The complete [G01–G18 register](REVIEW_GAPS_v0.22.md) retains Windows and physical
input/assistive qualification, uncoached sustained careers, hardware/storage
budgets, populated-community policy, production content/licensed geography,
migration, signed updates, public accounts/moderation, WAN/soaks, offsite RPO/RTO
and requirements-led distributed authority. They are acceptance projects with
explicit next proof, not silently closed by this increment.
