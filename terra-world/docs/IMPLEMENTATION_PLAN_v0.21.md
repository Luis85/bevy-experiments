# Terra World 0.21 — review, gap closure and polishing plan

Baseline: the user-supplied `terra-world-0.20.0(1).zip`, recovered and extracted on
29 October 2026. This plan was written after independent source/document reviews
and before implementation dispatch. The target deliverable is 0.21.0.

## Intended outcome

Make the existing regional reference game safer to operate and easier to recover,
remove two deterministic world/economy failures, and reduce unnecessary terrain
work. Preserve the forest-green native interface, central supply-contract funding
policy, supported source/asset identity rules, and save 1 / regional schema 1 /
wire 10 / asset schema 1 compatibility. A completed implementation must include
fresh regression evidence, an updated full gap register and a complete source
archive. A desktop review archive requires the existing canonical acceptance
gates to pass for its exact source and executable.

The source archive contains historical captures and receipts. Those describe
earlier runs; they do not establish current execution, physical-device support,
Windows acceptance, GPU performance, or production MMORPG capacity.

## Review coverage and method

| Owner | Perspectives and inspected evidence |
|---|---|
| Player-experience specialist | Product goals; launcher/session/recovery journeys; native UI, input and accessibility; `PRODUCT.md`, `DESIGN.md`, flow, controls, guidance and playtesting docs; native source and existing tests. |
| World/simulation specialist | Game design and finite economy; actor/vehicle integration; geospatial startup; streaming/render work, content and performance; domain/Bevy/source implementation and retained workload limits. |
| Reliability specialist | Storage admission, save privacy, crash recovery, operations, architecture and QA; regional worker/native adapters, snapshot publication, managed profiles and diagnostics. Independent technical UI assessment held until the design assessment completed. |
| Integrator | Network authority, replication and capability boundaries; canonical gate runner, CI and packaging; cross-module consistency, build feasibility, compatibility, documentation and final delivery. |

Reviews distinguish confirmed control-flow defects, documented product gaps,
proposed improvements and unverified concerns. The Impeccable context/detector
entrypoint is absent in this source environment; the real attempts failed with
exit 127. Product/design files were read directly. Source inspection does not
stand in for a rendered accessibility audit. Attempt current native capture only
through available, permitted execution; record any remaining limitation.

## Selected implementation work

### WP01 — Reachable loading and recovery controls (P1)

**Owner:** player experience. **Files:** `crates/terra-demo/src/flow.rs`, with
cross-module fixtures in `accessibility.rs` where needed.

`GameFlow::controller_menu_active` currently admits only Paused even though
Loading presents Cancel connection, Save and return, and Close failed world.
Admit actionable Loading overlays to controller focus and activation. Preserve
world-input blocking and the deliberately noninteractive Saving state.

Acceptance: controller-only activation reaches the actual network cancel and
terminal-storage close paths; local Loading controls are reachable; no world
intent escapes; normal paused/gameplay/saving behavior remains correct.

### WP02 — Isolate text editing and preserve readable recovery text (P1/P2)

**Owner:** player experience. **Files:** `accessibility.rs`, narrow parent-button
guards in `launcher/ui.rs` and `flow.rs`.

The on-screen text keyboard has no native modal tab boundary and underlying
menu observers can still accept activation. Reproduce this through event-level
tests, make the editing context genuinely modal, and guard parent actions while
the draft owns input. Keep Apply/Discard concise and preserve typed/controller
character input, exact target review, concurrent-change rejection and queue
capacity checks. Extend the existing word-or-character wrapping convention to
session and keyboard error/status text. Do not introduce a visual redesign.

Acceptance: Tab/Shift+Tab/Enter/Space cannot activate the underlying launcher,
session menu or world action while editing; Apply changes only its reviewed
target exactly once and restores focus; Discard preserves the original; existing
input isolation remains. Inspect minimum-window/large-text/long-path presentation
if native capture is available; otherwise report structural coverage only.

### WP03 — Explain the actual save contract (P2)

**Owner:** player experience. **Files:** `flow.rs`, `interface.rs`, one small
shared presentation helper if it removes duplication. Integrator updates docs.

Direct-play JSON currently receives a clean-exit-save promise implemented only
for managed worlds. Derive consistent copy from the real session: managed JSON,
direct-play JSON, regional checkpoints and server-owned persistence. Preserve
the documented manual save contract for direct play.

Acceptance: table-driven coverage for all four contexts; managed JSON names
clean-exit saving and no periodic autosave; direct-play JSON names explicit F5
or Save-and-exit actions; regional and network wording remains authoritative.

### WP04 — Heterogeneous merchants cannot crash maintenance (P1)

**Owner:** world/simulation. **Files:**
`terra-sandbox/src/settlement/economy.rs`, `terra-sandbox/tests/settlements.rs`.

Retail shortages are gathered from all shops, but priced using an unchecked
lookup into the first issuer's assortment. Valid distinct assortments can
therefore panic. Use checked issuer valuation, skip unsupported requests before
consuming offer capacity/IDs/escrow, and retain the central funding policy.

Acceptance: catalog-valid heterogeneous shops survive maintenance and save/resume;
unsupported requests consume no offer capacity or reward; supported requests
still post; cash plus reserved reward and goods are conserved; existing producer
priority and finite-career behavior remains.

### WP05 — Bootstrap from the real geographic neighborhood (P2)

**Owner:** world/simulation. **Files:** `terra-bevy/src/living.rs` and focused
headless integration tests.

The initial living-world count assumes a square neighborhood; tile latitude
clipping gives only six neighbors at a polar row for radius one, so a valid fully
loaded world waits forever for nine. Use the scheduler's actual clipped/wrapped
neighborhood and count relevant resident keys. Preserve the startup cap and
supported-ground requirement.

Acceptance: north and south radius-one neighborhoods initialize with their six
valid tiles; ordinary radius-one waits for nine relevant tiles; radius zero works;
unrelated residents and unsupported ground cannot falsely satisfy admission;
population, transport and frontier initialization remain integrated.

### WP06 — Remesh only chunks affected by source-feature edits (P2, G07/WP03)

**Owner:** world/simulation. **Files:** `terra-bevy/src/render.rs` and existing
GPU-free renderer tests.

Any removed-feature change currently increments one global revision and queues
all resident chunks. Bind feature-edit work to each chunk's effective removed
feature IDs, while retaining global invalidation for real quality-wide changes.
Use exact context rather than a collision-prone unchecked signature. Preserve
source-instance identity, LOD requirements, the two-worker budget and bounded
failure/retry behavior from 0.19/0.20.

Acceptance: one feature edit schedules only affected resident chunks; shared
boundary feature fragments all rebuild; unrelated meshes and in-flight results
remain useful; depletion, regrowth and edits during work cannot install stale
geometry; replacement/eviction/restore/quality/LOD/retry cases remain sound.
Report actual job counts separately from any unmeasured frame-rate effect.

### WP07 — Validate every new native regional activation (P2)

**Owner:** reliability. **Files:** `terra-bevy/src/regions.rs`, focused adapter
tests, and a corrected explanatory comment in `sandbox.rs` if necessary.

The native adapter bypasses store validation whenever live state is complete.
Submit the existing asynchronous hydrate operation for a new activation whether
state is complete or partial. Only an already admitted `activation_open` batch
may take the fast path. Pending jobs keep authority and queued work paused;
matching completion admits the batch; successful retirement/checkpoint closes it.

Acceptance: same-generation payload/schema corruption rejects before queued
commands or source registration mutate authority; valid live edits survive;
repeated calls submit one job per activation; multiple queued intents progress
and checkpoint without an infinite hydrate loop; generation-zero, full restore,
partial hydration, lost workers and stale completion revisions remain correct.

### WP08 — Keep JSON snapshots private during replacement (P2)

**Owner:** reliability. **Files:** `terra-bevy/src/sandbox.rs` and local-save tests.

Default temporary-file permissions can replace a 0600 snapshot with a 0644 inode.
Create publication temporaries as owner-only on Unix while retaining exclusive
creation, atomic replacement, file synchronization and cleanup.

Acceptance: first write and overwrite remain private under a permissive umask,
tested in an isolated subprocess; valid save/read round-trip succeeds; failed
publication preserves previous bytes and cleans owned temporary files. Do not
claim that Unix modes establish Windows ACL qualification.

### WP08b — Preserve evidence that a managed world has started (P2, R3)

**Owner:** reliability. **Approved after the initial review:** the cross-file
failure protocol below resolves the design dependency recorded in R3.
**Files:** `terra-demo/src/launcher/model.rs`, `terra-demo/src/main.rs`, native
regional startup admission and focused lifecycle tests.

Publish a private, immutable launch witness before the process can create or
save progress. The witness binds world identity, source and progress mode/path;
it means **started**, never **successfully saved**. Only the process that
exclusively creates the witness may initialize a genuinely unstarted world.
Subsequent launches require a validated existing checkpoint, including a
nonzero regional generation, even if an abrupt exit prevented presentation
settings from recording checkpoint presence. Legacy valid progress is validated
before gaining a witness. Rename and presentation-settings changes remain legal.

Acceptance: first save followed by abrupt termination and missing progress
rejects; corrupt/empty regional and JSON progress rejects without replacement;
genuinely unstarted worlds initialize once; competing first launches cannot both
initialize; witness publication/synchronization failure admits no save. An
interrupted launch before its first successful checkpoint deliberately requires
recovery or an explicitly separate new world on next launch. No automatic reset
is allowed. Existing save/wire schemas stay unchanged; older launchers do not
provide this new witness guarantee.

### WP09 — Bound stalled acceptance commands (P2 operational improvement)

**Owner:** integrator or a subsequently available delivery specialist.
**Files:** `scripts/verify.py`, focused Python execution tests and delivery docs.

`run_logged` currently waits without a per-command deadline. Add a configurable,
positive finite deadline with a conservative default, terminate the owned process
tree on timeout, retain its partial output and emit a visibly failed gate. Apply
bounded startup probes too. A timeout must never authorize packaging. Keep the
canonical gate roster, immutable terminal receipt and exact source/log hashes.

Acceptance: successful commands retain their bytes/result; a stalled command and
its child are stopped; partial logs and timeout classification survive; invalid
deadline input rejects; a failed/timed-out run cannot be admitted as successful.
Test actual process behavior, with supported-platform limits recorded.

### WP10 — Integrate, cross-review, validate and deliver (required)

**Owner:** integrator, with peer review after each owner finishes.

Update workspace/lock version to 0.21.0 and the generated compatibility table.
Archive the previous current validation/review byte-for-byte. Keep original
assets, data and historical evidence. Refresh README/PRODUCT/current review,
playtest guidance where behavior changed, this plan's completion record and the
full G01–G18 register. Distinguish code closure from hardware/content/service
qualification. Run focused regressions and the canonical full acceptance runner
against frozen implementation inputs. Deliver the complete source archive and,
only on successful exact acceptance, a Linux review package.

## Additional findings and required follow-up design

| Finding | Decision and completion criteria |
|---|---|
| First-session checkpoint witness (R3, P2) | Promoted to selected WP08b after the failure protocol was reviewed. A pre-launch witness closes the post-commit metadata gap and deliberately fails closed after an interrupted first launch. Closure still requires the stated lifecycle regressions; file-size inference is insufficient. |
| Directory publication durability (R4, P2) | Preserve as explicit follow-up unless a complete fault-injected implementation fits. Several helpers ignore parent-directory sync errors. Correct handling must distinguish pre-publication failure from uncertain publication after rename; it cannot promise rollback after publication. |
| Initial RTS actor/needs visibility (UX04, P2) | Valid product gap: default Activities does not continuously show selected owned actor and needs. Defer layout expansion until ownership/multi-selection/vehicle/remote-partial contexts and 1100×700 at 150% text can be visually accepted. |
| Assistive names and long-content layout | Source structure is useful evidence, but native screen-reader, text scaling, physical input and rendered clipping remain separately measured requirements. |

## Complete product gap coverage

| Stable area | Treatment in this increment / next proof |
|---|---|
| G01 Documentation | WP10: current version, traceability, evidence and historical preservation. |
| G02 Windows delivery | Existing Windows CI retained; actual Windows gates and clean-machine play/save/reopen/recovery remain required for the intended PC. |
| G03 Career progression | WP04 closes a maintenance crash; finite-career regressions retained. Broader strategy/balance and long uncoached play remain. |
| G04 Continuing goals | WP01–WP03 improve recovery and save understanding. Selected-actor visibility and uncoached goal comprehension remain. |
| G05 Navigation/recovery | Existing declared geometry/contact/progress regressions retained; denser moving traffic and extended imported geometry remain. |
| G06 Community lifecycle | Explicit unavailable-leader, dissolution, reclaim and treasury policy remains a product decision. |
| G07 Rendering qualification | WP06 closes broad feature-edit invalidation; physical GPU/frame-tail/residency measurements and arbitrary hung-worker containment remain. |
| G08 Accessibility/devices | WP01/WP02 improve logical controller and text-input paths; physical devices, assistive support, contrast, comfort and localization remain. |
| G09 Production content | Preserve original validated assets; production art/audio, boarding, passengers, DCC/retargeting and budgets remain. |
| G10 Regional data | WP05 closes clipped-neighborhood startup; licensed data, NoData/datum/topology and stacked-road geometry remain. |
| G11 Trusted delivery | WP09/WP10 strengthen bounded verification and exact artifact evidence; publisher signing, discovery, authenticated updates and rollback remain. |
| G12 Content migration | Exact identities retained; explicit migration conflicts, backup and rollback remain separate work. |
| G13 Public service controls | Reviewed protocol admission and private capabilities retained; enrollment, operators, moderation and deployment audit remain. |
| G14 WAN and capacity | Retain real local TCP/TLS checks; representative delay/loss, population targets and long soaks remain. |
| G15 Storage stalls | WP07 uses existing asynchronous bounded admission; transition/maintenance tail measurements and slow final disk operations remain. |
| G16 Operational recovery | WP07/WP08 plus R3 investigation; retention, offsite copies, ownership and recovery objectives need deployment-specific proof. |
| G17 Observability | WP09 preserves explicit timeout evidence; crash diagnostics, alerts, retention and operator ownership remain. |
| G18 Global architecture | Preserve the stated single-regional-authority boundary; partitioning follows measured population/data requirements. |

## Execution and acceptance rules

1. Dispatch WP01–03 to the player owner, WP04–06 to the world owner, WP07–08/R3
   and WP08b to reliability. Integrator owns WP09–10, manifests and current
   documentation. Coordinate overlapping hooks before editing; do not overwrite
   another owner's work.
2. Owners implement tests at the failing behavioral boundary and return exact
   changed files, outcomes and unresolved concerns. Shared Cargo runs are
   coordinated to avoid lock contention and misleading mixed-source evidence.
3. Cross-review changes for failure paths and integration, correct material
   findings, then freeze implementation inputs for the canonical 44 gates.
4. Native inspection uses one batched capture round for supported reference/minimum
   window and relevant states, one correction batch and at most one confirmation
   round. If blocked, state the concrete limitation; do not recycle old captures.
5. Verify final archive contents and preservation, retain actual logs/receipts,
   and state implemented scope, test results, platform limits and open gaps.

Questions skipped: the user has already authorized the review, planning,
delegation and implementation. No additional scope approval is needed for these
bounded corrections.

## Implementation completion record

Owners implemented WP01–WP09, including approved WP08b. Each owner then
cross-reviewed another scope. Root coordinated execution rather than allowing
competing Cargo runs or presenting mixed-source candidates as acceptance.

| Work | Completion and material review correction |
|---|---|
| WP01–WP03 | Loading actions, native modal input ownership, wrapping and four-mode save description implemented. Actual deferred Activate fixtures were corrected with queue flushing and positive controls. Launcher review now reuses the same save description. |
| WP04 | Checked central issuer valuation preserves supported offer IDs/capacity/funds; heterogeneous-shop conservation and continuity regression implemented. |
| WP05 | Shared actual neighborhood reaches living bootstrap and native Loading. Cross-review added exact current source-integrated terrain membership and kept full-neighborhood smoke capture policy. |
| WP06 | Exact per-chunk effective feature context preserves unrelated work/geometry, including 1-of-49 and shared-two-fragment regression measurements. |
| WP07–WP08 | New native activation batches pass asynchronous validation; Unix snapshot temporaries are private from creation. Validated restore admission remains a legal existing batch. |
| WP08b | Exclusive pre-launch record and required-progress barriers implemented. Cross-review added profile/witness ancestor, alias, lock and repeated runtime export protection. |
| WP09 | Gates and startup probes share bounded logged execution. Cross-review fixed orphaned startup descendants/partial output loss and qualified unconfirmed Windows cleanup. |
| WP10 | Updated documentation and complete G01–G18 register; 0.20 current reports archived unchanged. Focused and native results retained separately from final canonical acceptance and delivery. |

Focused confirmation passed 203 demo tests, 16 filtered mesh tests, six regional
adapter tests, four permitted snapshot-filter tests and 34 Python tests. Earlier
candidate failures are retained and explained in Validation. Final counts,
including all deduplicated identities and repeated executions, must be taken
from the canonical completed receipt referenced by [Validation](VALIDATION.md).

Fresh native inspection found one Space-key wrap at 1100×700/150% text. One
correction batch widened that key without reducing text, and one rebuilt
confirmation round verified it. Actual managed create/checkpoint/save-and-quit
was followed by a final-binary reopen/render/save-and-quit with the same witness.
Process/source/image evidence and limitations are in
[the native record](verification/v0.21-native/README.md). Logical/native Linux
evidence does not close Windows or physical/assistive-device criteria.

R4 directory-sync uncertainty and UX04 selected-actor/needs visibility remain
explicit follow-ups. No production content, service, platform or hardware gap is
declared closed merely because a scoped implementation or regression passed.

Final canonical execution completed successfully in
`verification/v0.21-final-acceptance`: **44 gates, 1,000 distinct Rust tests,
1,097 passing Rust test executions and 34 separate Python tests**. No Rust test
failed or was ignored, and no accepted gate timed out. All 373 frozen inputs,
logs and reports retained their accepted identities. The first canonical
candidate was deliberately interrupted to complete the desktop documentation
allowlist; it is retained separately and does not authorize packaging.
