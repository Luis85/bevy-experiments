# Open gaps and improvement follow-up — 0.14

Review date: **2026-10-28**. This increment follows the accepted
[0.13 gap report](REVIEW_GAPS_v0.13.md), with its
[product review](PRODUCT_REVIEW_v0.13.md) and
[validation record](VALIDATION_v0.13.md) preserved unchanged. The original
[0.12 report](OPEN_GAPS_REPORT_v0.12.md) and
[PDF](reports/Terra-World-0.12-Open-Gaps-Report.pdf) remain the baseline for the
stable **G01–G18** identifiers below.

**Source and native acceptance passed:** all **26 frozen-source gates**, with
**828 distinct Rust tests**, **895 passing executions**, zero failed or ignored
tests and **347 unchanged inputs**. The accepted executable also passed the
Company confirmation, both restored-world launches and controlled interruption/
restart checks. See the [gate record](verification/v0.14-final/gates.json),
[native record](native-v14-verification.json) and [validation](VALIDATION.md).
Desktop packaging and extracted native checks are documented separately in the
[package record](desktop-package-v0.14.json) and
[release record](release-verification-v0.14.json).

## Outcome and scope

This pass addresses four practical problems in the regional reference game and
SDK: returning company owners lacked service-specific goals; artists had to
assemble model hashes and manifests manually; a database backup did not collect
the source/configuration needed to reopen its world; and native diagnostics
could disappear when a process stopped before graceful exit.

The new Company goal, recipe-driven asset tooling, complete local recovery sets
and retained diagnostics build on existing authority and content contracts.
Three implementation workstreams were cross-reviewed, with further integration
review covering the default native world configuration, output destinations,
financial explanations and restored UI context. The
[product review](PRODUCT_REVIEW.md) explains the experience across players,
artists, developers, operators and support.

The increment does not change gameplay save version **1**, regional schema **1**,
wire protocol **10** or TLS **terra-world/10**. Presentation packs retain their
runtime format and do not change saved world authority. Original terrain and
wildlife inputs remain the reference content. See
[Compatibility](COMPATIBILITY.md) for the current contract.

## Improvements and their evidence

### Continuing company goals and usable recovery handoffs

Activities adds **Run your transport company**. Before dispatch it follows real
registration, funding, recruitment, route comparison and vehicle assignment.
For an existing service it names the actual driver, route, next stop or return
pickup, completed paid deliveries and company cash. Services requiring attention
take precedence; within that priority, a loaded obligation comes before an empty
waiting service. Suggested journey still addresses critical needs and accepted
personal work first, while an explicitly selected track remains explicit.

The primary action opens the reviewed Fleet section and selections. An expired
rental opens that exact vehicle in Garage. The guide distinguishes payroll from
rental time, personal from company money, and a safe pause after delivery from
an immediate hold that retains the load. It explains finite stock, queues,
issuer/buyer funds and accounting limits. Opening a panel neither spends money
nor resumes, returns, releases or moves a vehicle.

The held-service handoff opens Recovery. Its explanation now names the actual
**Resume delivery / return** control there, keeping the advice aligned with
the panel the player receives.

Displayed handoffs bind the owner, driver, vehicle, route, facility, next stop,
status and commitment. Native guidance also binds its displayed player and
restore revision. A changed assignment or restored world requires a fresh
review; ordinary movement and payroll accrual do not unnecessarily invalidate
a read-only handoff. Unrelated Fleet and Garage confirmations are cleared.

The regressions cover paid setup, loaded hold/save/resume, blocked-source advice,
urgent and accepted-work precedence, owner privacy, stale identities, rental
renewal, capacity/money limits and legacy accounting absence. All nine new
domain cases and the native integration cases passed. The prepared review
fixture starts near a garage and takes one ordinary 0.05-second approach step,
then uses the starting wallet and paid commands and drives to an actual
passenger pickup before holding the load. It supports a later-state UI review;
it is not a substantial walking benchmark or an uncoached playtest. The
[journey record](verification/v0.14-native/company-review-journey.json) records
those limits. Full generated review worlds remain private temporary inputs.

Trace: [Company guidance](../crates/terra-sandbox/src/guidance/company.rs),
[domain tests](../crates/terra-sandbox/tests/company_guidance.rs),
[native handoffs and prepared journey](../crates/terra-demo/src/frontier.rs),
[Guidance](GUIDANCE.md) and [Companies](COMPANIES.md).

### A repeatable model-authoring handoff

`terra-assets inspect MODEL.glb` reports actual nodes, named clips, durations,
channels, hashes and importer budgets. `build RECIPE.json NEW_DIRECTORY` reads
explicit model sources, bindings, animation policies and provenance; snapshots
the bytes; computes content-named outputs; and runs the complete existing
runtime validator before publication. The destination must be new. Source
exports and existing packs are never updated by this workflow.

The output contains the runtime `pack.json`, accepted GLBs and an authoring
report that binds the recipe, source provenance and resulting pack identity.
Identical inputs produce identical outputs. Separate bindings keep their own
animation policy even when they share model bytes. Original allocation,
hierarchy and unused-animation checks remain in force.

The included recipe uses the five unchanged original wildlife models. It gives
developers a complete export-to-runtime example without replacing character
customization. The guide provides units/axis conventions, clip mapping, supported
export settings and corrective actions for rejected content. Author-declared
provenance is retained; it is not publisher authentication or license review.
Production human, cabin, seated/boarding, customization attachment and audio
content remains open.

The final inspection, recipe build and runtime revalidation gates passed. The
[generated pack](verification/v0.14-final/asset-authored-validate.json) contains
five admitted models; accepted-executable
[telemetry](verification/v0.14-native/v14-confirm-prepared-company-telemetry.json)
shows 22 ready instances, zero failed models and zero rejected instances.

Trace: [authoring API](../crates/terra-bevy/src/asset_authoring.rs),
[CLI](../crates/terra-bevy/src/bin/terra-assets.rs),
[authoring regressions](../crates/terra-bevy/tests/asset_authoring.rs),
[working recipe](../crates/terra-bevy/assets/wildlife-authoring.recipe.json),
[Authoring guide](ASSET_AUTHORING.md) and [runtime contract](ASSET_PACKS.md).

### Recovery with the dependencies needed to reopen the world

`terra-worldctl bundle-create`, `bundle-verify` and `bundle-restore` provide a
private recovery directory for the reference local regional host. It retains a
consistent committed SQLite snapshot, the exact built-in catalog, resolved
configuration, all prepared-source files, and an optional bounded local political
GeoJSON input. Procedural worlds record their supported generator revision.
Saved authored political changes remain separate in the exact host metadata.

Verification checks the complete inventory, byte digests, prepared-source
identity and coverage, catalog, SQLite integrity, complete domain, local host
metadata and committed archive. Restore requires a new destination, preserves
the committed generation, and rebases runtime paths into the restored directory.
An independently retained expected bundle identity detects substitution of the
whole set. It does not authenticate a publisher or encrypt a backup.

The final [rehearsal](verification/v0.14-final/recovery-bundle.json) passed all
15 verification flags after removing the original database/configuration/source
tree. It restored generation 2 across 1,029 nonempty regions, with 4,096
resources, two principals, owned bodies, a faction and shared property. The
comparison retained complete cold state, local metadata, source/catalog identity,
original political bytes and authored politics; a damaged copy rejected without
restore output. Distant authored resources do not establish terrain coverage at
every location.

Separate [prepared Company](verification/v0.14-native/v14-prepared-recovery-summary.json)
and [ordinary menu-world](verification/v0.14-native/v14-menu-recovery-summary.json)
rehearsals removed their original test inputs and retained byte-exact full
exports after restore. Both then reopened with the accepted executable from
absolute restored configurations in an unrelated working directory. The menu
world rendered 49 chunks with its political input; the prepared world retained
its deliberately politics-free configuration.

Integration review extended the initial design to include the political input
used by ordinary native menu worlds. The new `--no-default-politics` launch
option also prevents a deliberately politics-free restored configuration from
acquiring the demo's implicit default dataset. Explicit political inputs remain
honored. Live sources, historical packs, custom catalogs and server databases
still require their own host dependency policies.

Trace: [recovery API and regressions](../crates/terra-bevy/src/recovery.rs),
[operator CLI](../crates/terra-bevy/src/bin/terra-worldctl.rs),
[rehearsal](../crates/terra-bevy/examples/recovery_bundle_probe.rs),
[Recovery bundles](RECOVERY_BUNDLES.md) and [Operations](OPERATIONS.md).

### Retained incident evidence before exit

`--telemetry FILE --telemetry-interval-seconds N` enables periodic native
captures on a real-time cadence, including while the virtual clock is paused.
One background writer accepts one in-flight and one queued sample. Busy periods
skip new samples instead of accumulating an unbounded queue. The file retains
the latest **16 successful captures across runs**, with run identity, sequence,
timestamp, elapsed time and running/graceful/error exit status.

Samples are bounded to **64 KiB** and the complete document to **2 MiB**.
Publication synchronizes a temporary file before replacement; failed writes
retain the previous complete report. Final export runs after the world checkpoint
barrier and a failed requested final export returns an error exit. An unrelated
file, invalid history, symlink destination or save/database/session alias is
rejected, including output ancestors that would occupy a future journal path.

A controlled interruption of the accepted executable left diagnostic evidence:
the [SIGKILL record](verification/v0.14-native/v14-interruption-event.json)
retains 15 readable `running` samples. Restarting with the same report path
kept 14 prior samples plus two new ones, bounded to 16, with the latest marked
`graceful_exit`. The [continuation record](verification/v0.14-native/v14-interruption-continuation.json)
confirms world resume, exit status 0 and no world, simulation or storage error.
This checks process termination and restart, not physical power loss.

A surviving `running` sample after interruption is diagnostic context. Its
timestamp shows where evidence stopped; missing or stale evidence is unknown
health. Reports contain timings, counts and errors rather than inventories,
balances or resume capabilities. Error paths and source identifiers can still
require review before sharing. This supplies bounded local retention, not crash
dumps, external alerts, continuous traces or an operator audit service.

Trace: [retained writer and tests](../crates/terra-demo/src/telemetry_capture.rs),
[native capture integration](../crates/terra-demo/src/performance.rs),
[Diagnostics](DIAGNOSTICS.md) and [Performance](PERFORMANCE.md).

## Review and polishing corrections

These findings were corrected during cross-review. Their regressions passed in
the frozen-source gates; the final native paths confirmed the integrated
Company, recovery, presentation and diagnostic behavior.

| Finding | Correction and product consequence |
|---|---|
| A recovery destination could occupy a database's absent WAL/SHM/journal name. | Check the protected database namespace before claiming an output; a backup operation cannot make the incident database unreopenable. |
| A restore inside its retained bundle was rejected only after copying. | Reject nested destinations before writing, keeping the retained set untouched. |
| Native path joins could create nonportable archive member names on Windows. | Construct canonical `/` member paths while retaining rejection of untrusted backslashes. Windows execution still needs qualification. |
| “Destination full” advice assumed a load always existed. | Distinguish empty pickup, loaded delivery and return/refund failures; do not offer a nonexistent return or erase an obligation. |
| A payroll hold always suggested more company funds. | Recognize the employee wage-wallet limit and avoid promising that a deposit fixes every payment failure. |
| Matching service IDs could survive a restore while the reviewed world changed. | Bind the displayed guide to the principal and restore revision. |
| The initial recovery scope excluded the default game's political source. | Retain the bounded base input and exact authored saved state, with explicit native handling of politics-free configurations. |
| Recursive diagnostic output creation could occupy a future save or journal path. | Validate all output ancestors, preserving the original world/session namespace without creating a conflicting directory. |
| Screenshot and political exports had the same parent-directory risk, and exports could collide with a current or future multiplayer session file. | Apply ancestor protection to screenshot/political outputs and session-namespace protection to every export kind before startup. Three focused regressions cover these cases in [output guards](../crates/terra-demo/src/output.rs), integrated through [startup](../crates/terra-demo/src/main.rs). |
| Held-service advice referred to a different panel's Resume control. | Name Recovery's actual “Resume delivery / return” control in the held-service explanation. |

The applied polish preserves the Activities/Fleet layout and places the action
before detailed explanation. Candidate review covered 1440 × 900 and the smaller
layout, retaining nine compared subsystems exactly while streaming could add
other environmental records. The accepted-executable confirmation repeated the
Recovery handoff, focus, scrolling, named review/cancellation, F5/F9 and
save-and-quit at 1100 × 700 with 150% text. Its
[complete before/after export was byte-exact](verification/v0.14-native/v14-confirm-company-conservation.json),
and exit status was 0. The [focused Company view](screenshots/v14-confirm-company-focus.png),
[recovery action](screenshots/v14-confirm-recovery-action.png) and
[canceled review](screenshots/v14-confirm-review-cancelled.png) retain the actual
screen evidence. This bounded native confirmation does not establish complete
accessibility acceptance.

## Full gap register and completion criteria

“Scoped improvement” identifies the implemented problem or bounded workflow.
It does not close a larger baseline gap while its completion criteria remain
unmet. P1 refers to the named release audience or promise; Internet-service
priorities are conditional on deploying that service.

| ID | Status after this increment | Remaining closure criteria |
|---|---|---|
| **G01 — Documentation** | Scoped improvement: authoring, recovery, diagnostics and continuing Company guidance now have connected contracts and review records. | Keep source-based compatibility checks, current-guide review and final artifact/link verification in every release. Historical evidence must retain its original values. |
| **G02 — Windows delivery** | Portable archive naming corrected; platform qualification remains open. **P1 for Windows players.** | Execute Windows CI and a clean-machine creation/play/save/reopen/recovery journey, including the new tools. Declare OS/GPU prerequisites and signing, installer/update/removal policy. |
| **G03 — Career progression** | Service costs and finite limits are clearer; economic rules are unchanged. Broad balance remains open. **P1 for a career release.** | Validate additional beginner/advanced strategies, setbacks, living/labor/capital costs, treasury replenishment and longer player sessions. Retain finite managed-career limits when reporting prior measurements. |
| **G04 — Continuing goals** | Scoped improvement: state-derived Company setup, returning-service briefing and exact recovery handoffs. **P1 for sustained play.** | Observe uncoached first-session and returning-player Supply, Home/Community and Road/Company journeys. Measure misunderstood costs, stalled actions and recovery success; add sustained content where those observations require it. |
| **G05 — Navigation/recovery** | Recovery discoverability improved; no new general route-planning or unattended-autonomy claim. **P1 for denser regions.** | Exercise multiple/moving obstacles, narrow roads, blocked junctions, alternate exits and missing/restored terrain. Require bounded physical progress or a usable value-preserving recovery action across declared cases. |
| **G06 — Community lifecycle** | Accepted 0.13 departure/removal/handover behavior retained; broader governance remains open. **P1 for long-lived groups.** | Define unavailable-leader succession, dissolution, territory transfer/reclaim and treasury policy, then verify offline permissions and durable value preservation. |
| **G07 — Rendering qualification** | Validated authoring uses existing import/admission budgets; hardware qualification remains open. **P1 for performance promises.** | Profile CPU/GPU frame tails, actual residency and loading peaks on chosen hardware. Budget direct game-owned scene overrides or require validated metadata. Import limits are not GPU measurements. |
| **G08 — Accessibility/devices** | Accepted-executable handoffs, enlarged-text focus, scrolling and cancellation confirmed; broader device/accessibility acceptance remains open. | Qualify complete keyboard/controller/wheel journeys, reconnect/device ordering, assistive access, contrast, motion comfort and localization with representative users. |
| **G09 — Production content** | Scoped improvement: GLB inspection, recipe build, provenance/reporting and a working original wildlife handoff. | Produce representative human/customization, vehicle/cabin, seated/boarding and audio content. Complete DCC presets, attachments, licensing, LOD/material budgets and native transition/visibility checks. |
| **G10 — Regional data** | Original connected prepared inputs retained; licensed shipping-region ingestion remains open. | Select shipping data and reproducible ingestion; resolve NoData, datum, topology, complex relations and bridge/underpass geometry. Verify real coverage and provenance. |
| **G11 — Trusted delivery** | Local immutable outputs are easier to create and verify; publisher trust remains open. | Define signing/trust, release discovery, authenticated transfer, interruption/resume, installed-version selection and rollback. Content hashes and provenance declarations do not establish publisher identity. |
| **G12 — Content migration** | Exact recovery is implemented for its supported scope; migration remains open. | Define ID/geometry/catalog conflicts affecting plots, resources, homes and obligations. Provide deliberate migrations with backups, rollback and exact post-migration validation. Restore must not silently migrate. |
| **G13 — Public service controls** | Open. **Conditional P1 for Internet hosting.** | Provide account enrollment/recovery, authenticated operator access, durable moderation/audit policy and abuse controls. Local recovery bundles do not replace server capability or public-account validation. |
| **G14 — WAN and capacity** | Existing finite local workload remains the boundary; WAN/capacity qualification is open. **Conditional P1.** | Measure sustained representative clients under delay/loss and TCP head-of-line stalls, plus long soaks. Set supported populations and frame/acknowledgement targets from those results. |
| **G15 — Storage stalls** | Periodic diagnostic writes are bounded and offloaded; authority storage performance remains open. | Measure transition-pause distributions and tails; reduce main-thread clone/install and mutation stalls while preserving complete validation and acknowledgements after durability. Final maintenance can still wait on storage. |
| **G16 — Operational recovery** | Scoped improvement: private local database/catalog/config/source/politics sets and new-location restore with a finite original-tree-loss rehearsal. Deployment recovery remains open. **Conditional P1.** | Set retention, confidentiality, offsite copies, recovery owners and RPO/RTO; rehearse transfer, matching application availability and native reopening in the intended deployment. Extend dependency/metadata policies deliberately for server, historical, live and custom-catalog worlds. |
| **G17 — Observability** | Scoped improvement: periodic retained native samples, explicit interruption status, bounded writer and incident-triage guide. | Add deployment retention, crash diagnostics, external audit retention, actionable alerts and operational ownership. Qualify failure detection; a surviving sample or missing final export is not a health certificate. |
| **G18 — Global architecture** | Conditional expansion; no new global-scale claim. | Define real population/data requirements before adding authority partitions, cross-shard ownership/economy and adaptive content delivery. Regional paging remains a single-authority technique. |

## Verification boundary and next priorities

The completed release gates cover domain behavior, freshly rerun commercial
careers, source/persistence/TLS contracts and the native build, plus the authoring
inspection/build/runtime-validation chain and complete recovery rehearsal.
Native records separately identify the candidate review and accepted-executable
Company confirmation, generated pack, restored-world startup and interrupted
session/restart. Desktop package integrity and extracted native execution have a separate
[record](desktop-package-v0.14.json), scoped to the recorded Linux host. Windows, target hardware, assistive technology
and uncoached player studies cannot be inferred from these checks.

The next player-release priority is Windows/target-hardware qualification and
observed new/returning play. Use those sessions to decide whether longer goals,
financial pacing or navigation failures most often stop progress. The next
content priority is one representative human/vehicle/boarding production slice
through the now-repeatable authoring workflow and one licensed regional dataset.

For deployment, build on the local recovery unit with a separately retained and
verified offsite copy, a named operator and a timed restore rehearsal against
chosen RPO/RTO. Public accounts, WAN capacity, audit retention and alerting need
their own acceptance programme. Expanding global architecture should follow
measured regional requirements rather than substitute for these remaining
player, content and operating gaps.
