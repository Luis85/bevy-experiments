# Documentation gap and claim audit — 0.15

Reviewer: navigation_increment_015. Project: `/workspace/scratch/0fdc9aa7bc4c/work/terra-world-next`.

## Release judgment

The implemented 0.15 work makes **scoped contributions to G05, G09 and G16**, with related documentation, handoff and interaction improvements in G01/G08. It does not close the original G01–G18 register as a whole. The important distinction is between a supported bounded implementation, an executed regression, an observed native journey, and broader platform/player/deployment qualification.

This audit reviewed README, PRODUCT, DESIGN, the current review/validation and compatibility guides, the 0.14 register, asset authoring/import guides, navigation/handling contracts, game flow, desktop delivery, recovery, operations, performance/scaling and the relevant source and focused evidence. The parent was actively authoring the 0.15 review while this audit ran. Old content in the mutable review/validation pages is recorded as a release-finalization requirement, not evidence that a 0.15 gate failed. No Cargo commands were run.

## Actionable documentation findings

### D01 — Replace mutable release summaries only with actual 0.15 evidence

At the reviewed snapshot, `docs/PRODUCT_REVIEW.md:1–18` and `docs/VALIDATION.md:1–29` still described accepted **0.14** results, while README and the checked compatibility table already named **0.15**. The historical 26 gates / 828 distinct tests / 895 executions / 347 inputs / accepted executable hash must not appear as 0.15 results. The only missing relative link found by the current-guide target scan was README's planned `docs/REVIEW_GAPS_v0.15.md`.

**Parent-owned follow-up:** publish the current 0.15 product review, validation and complete gap register after each named source/native/package result exists. Preserve every versioned 0.14 document and evidence byte. Use explicit pending/not-executed status for any missing gate or platform. Link accepted native evidence to the actual accepted executable and distinguish candidate UI review from final confirmation.

### D02 — Historical measurement links must remain attached to their measured release — corrected

`docs/SCALING.md:458` asserted the 0.12 counts (20 gates, 747 distinct tests, 327 unchanged inputs) while linking mutable `VALIDATION.md`. `docs/OPERATIONS.md:491` linked 0.12 incomplete-attempt details to an anchor in the mutable validation page. These become misleading or broken every time the current release summary changes.

**Applied, with parent authorization:**

- SCALING now links `VALIDATION_v0.12.md`, explicitly labeled the 0.12 validation record; no measured value was changed.
- OPERATIONS now links `VALIDATION_v0.12.md#earlier-incomplete-attempts`, labeled 0.12 validation; the target heading was verified.

No historical document was edited.

### D03 — Unchanged schema 1 does not mean older importers accept new pack features — corrected

The asset guides correctly say that `vehicle_model` targets and `animation.vehicle_motion` need Terra 0.15 or later. The canonical compatibility handoff previously said only that saves/schema/protocol remain unchanged. A developer using that handoff alone could reasonably confuse domain/save compatibility with presentation reader compatibility.

**Applied, with parent authorization:** a new explanatory section in `docs/COMPATIBILITY.md`, outside the generated marker, states the 0.15 reader requirement, older-reader rejection, unchanged normalization/identity of existing kind/species packs, and absence of a serialized field when wheel policy is absent. It also distinguishes the native schema-1 `world-backup.json` + `bundle/` wrapper from a bare operator bundle and from the domain/database schema.

**Verification:** `python3 scripts/check_compatibility.py` passed for all nine workspace packages. Release 0.15.0, wire 10, ALPN `terra-world/10`, domain save 1, database schema 1, Rust minimum 1.97.1 and pinned acceptance compiler 1.99.1 agree with source. All relative link targets in the three edited guides exist. The generated compatibility table was not changed.

### D04 — Current navigation inventories omit usable destinations — delegated

- `docs/GAME_FLOW.md:121–122` lists Supply, Home, Community and Road in the Activities cycle, omitting the implemented Company goal.
- `docs/GAME_FLOW.md:332–333` omits `backups` from allowed `--menu-screen` values, although `launcher/mod.rs` accepts it.
- `DESIGN.md:66–67` omits Backups and RecoveryReview from the launcher screen inventory.
- PRODUCT and DESIGN stop their versioned principles at 0.14 and do not yet describe the bounded cluster, model-specific rig or reviewed native recovery contract.

**Ownership:** launcher owner was asked to correct the two GAME_FLOW omissions while editing that guide; parent owns DESIGN/PRODUCT updates. These are current-guide corrections, not changes to old release evidence.

### D05 — Unsupported recovery states name workflows that are not concrete enough

The launcher says Portable JSON worlds use a separate file-copy workflow and historical/live sources need an operator recovery workflow (`launcher/recovery.rs`, `unavailable`). `GAME_FLOW.md:159–160` repeats that separate workflows are explained. However, RECOVERY_BUNDLES explicitly rejects live/historical/custom-catalog/server inputs, and the current guides do not provide a concrete managed portable-JSON recovery procedure. Generic database backup/export remains available, but it is not a complete historical/live dependency restore policy.

**Small, useful closure within this increment:** add a short unsupported-world handoff table to the recovery guide and align UI wording. For Portable JSON, state exactly what must be retained (complete snapshot, profile/configuration, required original source/political inputs, matching application), and distinguish a retained copy from the automated managed recovery feature. For historical/live/custom worlds, say that the complete dependency policy is not supplied by this local-bundle workflow, then link generic database/export tools without implying that those tools reconstruct missing dependencies. Do not invent a working migration or a safe automatic import that has not been implemented and verified.

This is a product-help gap, not a request to expand recovery formats during the final polish pass.

### D06 — Make the optional Comet demonstration's limitations visible in release copy

The authoring/import guides are precise: Comet binds the existing compact model; the composed optional pack contains the five unchanged wildlife models plus Comet; the default woodland pack remains unchanged. Imported exteriors hide in first person, which retains the procedural cockpit. Empty cabin seats do not represent passengers; the fixed livery does not reflect saved paint. Skin/customization, boarding, attachment, audio, LOD and hardware work remain open.

**Parent-owned acceptance:** preserve these distinctions in the release report and screenshot captions. Do not call this a production human/boarding library, a new default vehicle fleet or a replacement first-person cockpit. One model's validated budgets are not measured frame time or physical GPU residency. The direct demo pack and recipe-built pack legitimately have different identities because their normalized declarations/paths differ; retain both exact evidence identities separately.

### D07 — Some active pointers still read as the latest review

`docs/DESKTOP_DELIVERY.md` ends with a 0.14 follow-up link. PRODUCT's general release-principle paragraph links the 0.12 register. `docs/MOTORTOWN_PARITY.md` explicitly dates its benchmark to 0.12, so its numeric/historical review must remain dated rather than silently rebranded as a new external-product audit.

**Follow-up:** point current-priority links to the 0.15 register once published, while preserving explicitly historical benchmark/source links. Keep the previous external benchmark review date and describe new Terra deltas separately if updating that guide. The current-guide path scan found no other missing file target at audit time.

## Product gaps worth closing before the 0.15 freeze

1. **Completed-result reachability (P2 already with launcher owner).** The bounded 24-entry/256-folder rescans can omit a just-completed backup or recovered world while the success notice promises it in the corresponding browser. Retain an exact session-level output pin through navigation/refresh without widening scan bounds. A deterministic regression uses 24 valid future-dated entries (e.g. after clock correction). See `review-launcher-015.md` for the independent code review and exact reproduction. Owner has confirmed the fix and regression approach.
2. **Unsupported recovery handoff copy.** Close D05 so an unavailable button gives a truthful next step and does not send a player into another unsupported bundle operation.
3. **Current entry-point consistency.** Close D01/D04/D07 and keep the asset reader feature boundary explicit (D03 is already corrected).
4. **Actual integrated native journeys.** Use the accepted executable to prove that the new backup result can be found/reviewed/recovered/reopened after original source-tree loss and that Comet model selection, signed rolling, stop/pause, fallback and first-person visibility survive the real renderer. Screenshots alone cannot establish exact world-state preservation; keep the state comparison alongside them.

Broad balance, production humans, general road deadlock planning, signed offsite delivery, public accounts, Windows hardware and distributed authority should not be fabricated as quick closure for this increment.

## Proposed G01–G18 status delta

These are proposed current statuses, not replacements for historical rows. Attach final 0.15 evidence only after its gate/inspection has actually completed.

| ID | Proposed 0.15 status delta | Precise remaining acceptance criteria |
|---|---|---|
| **G01 — Documentation** | Scoped improvement: current cluster, transport authoring, native recovery and explicit presentation-reader compatibility contracts; historical measurement pointers corrected. Release handoff remains pending until all current reports/links match final evidence. | Check current guides and commands against source; publish the 0.15 18-row register, source/native/package identities and actual test counts; preserve historical bytes and original measurements; verify final archive links and included report. Repeat every release. |
| **G02 — Windows delivery** | Unchanged open **P1 for Windows players**. CI/package configuration is not an executed Windows playtest. | Run Windows CI and a clean-machine source/package startup, creation, paid play, save/reopen, native backup/recovery, content load/fallback and TLS resume journey. Record OS/GPU/driver/input devices/runtime dependencies/package hashes; establish distribution/signing/update/removal policy. |
| **G03 — Career progression** | No price, funding or balance change. Better physical passage may remove one stall but does not establish a sustainable career. Prior finite managed-career evidence remains scoped. | Measure beginner/advanced strategies including living, labor, rental, capital and input costs, setbacks and replenishment over longer sessions; report interventions and ending obligations exactly. Rerun original careers without changing fixtures or treating injected treasury as earnings. |
| **G04 — Continuing goals** | Existing state-derived Supply/Home/Community/Road/Company guidance retained; native backup recovery adds a practical continuity handoff. No new campaign or uncoached-play closure. **P1 for sustained play.** | Observe uncoached first and returning sessions across all named tracks; record stalled actions, misunderstood costs and failed recoveries; measure successful ordinary-command completion and add sustained content where observed needs require it. |
| **G05 — Navigation/recovery** | Scoped improvement: bounded outer-corner fallback around up to four encountered stopped bodies, with a three-parked-body progress case, opposite-side support, missing/restored support, moving-obstacle safety, patrol and exact-resume regressions. General congestion remains open **P1 for denser regions**. | Extend declared cases to multiple moving obstacles, narrow constrained roads, blocked junctions/opposing flows, alternate exits and original loaded company obligations. Require actual supported/contact-clear physical progress within a stated deadline or an actionable value-preserving hold/recovery. Keep source/work/path limits and exact resume; do not infer general route completeness from an authored flat case. |
| **G06 — Community lifecycle** | Unchanged accepted departure/removal/handover behavior; long-lived governance remains open **P1 for long-lived groups**. | Define unavailable-leader succession, dissolution, territory transfer/reclaim and treasury disposition; verify offline authority revocation, durable property/funds and stale-review rejection for each lifecycle action. |
| **G07 — Rendering qualification** | Scoped correctness addition: model-specific rig work is admitted/budgeted and animation follows actual motion/pause. Target hardware qualification remains open **P1 for performance promises**. | Measure CPU/GPU frame tails, actual residency/loading peaks and transparency/overdraw in dense/day/night/weather/cockpit scenes on named hardware; qualify fallback under pressure. Cover trusted direct scene overrides with metadata or explicitly keep them outside validated budgets. |
| **G08 — Accessibility/devices** | Scoped launcher interaction extension: named recovery states, scrolling, exact review identity and background progress; actual native keyboard/small-window confirmation is separately required. Broader qualification remains open. | Complete keyboard-only and physical controller/wheel creation/play/save/recover flows, focus/scroll/error/cancel states at 1100×700 and enlarged text, reconnect/device order, assistive access, contrast/motion comfort/localization with representative users. Virtual input and Linux software Vulkan are limited evidence. |
| **G09 — Production content** | Scoped improvement: one original Comet exterior/cabin/wheel-rig model, specific catalog binding, reproducible generator, direct six-model pack and composed recipe. Original wildlife and procedural customization remain intact. | Finish representative human/customization and attachments, real seated/boarding transitions, dynamic livery/material behavior, audio, DCC presets and LOD budgets. Verify native loading, forward/reverse/stop/pause, first-person hide/fallback and saved-world reopen; then qualify content on actual target hardware. |
| **G10 — Regional data** | Unchanged connected prepared terrain and source identities. Real licensed shipping-region ingestion remains open. | Select licensed data; reproduce ingestion with datum/NoData/topology/complex relation/bridge-underpass policies; inspect actual geography, continuous support/coverage and provenance. Synthetic coordinates and connected fixture seams are not a real shipping-region survey. |
| **G11 — Trusted delivery** | New local pack/recovery workflows retain content identities; publisher trust remains open. | Define signing/trust roots, release discovery, authenticated transfer, interrupted-download resume, installed-version selection and rollback; rehearse rejection and recovery. Hashes, canonical metadata and provenance declarations do not authenticate a publisher. |
| **G12 — Content migration** | Exact same-content restore and no-authority-change presentation extension retained. Source/catalog/geometry migration remains open. | Define conflict policy for changed IDs, geometry/catalog, plots, resources, homes and commitments; provide explicit migration, pre-change backup, rollback and full post-migration validation. A schema-1 save or restore must not silently adopt another source/catalog. |
| **G13 — Public service controls** | Unchanged open **conditional P1 for Internet hosting**. Native local backups are not server account or capability recovery. | Implement/qualify enrollment and account recovery, authenticated administration, abuse/moderation rules, durable audit ownership and failure handling. Exercise real deployment identities and authority recovery beyond local bundle validity or TLS. |
| **G14 — WAN and capacity** | No new WAN/population promise. Existing finite local TLS workload remains the reference; new cluster/recovery tests do not raise capacity. **Conditional P1.** | Run sustained representative populations under declared latency/jitter/loss and TCP head-of-line stalls, dense-interest transitions and long soaks; publish tick/ack/frame tails, bandwidth, memory plateau and overload behavior; set supported counts only from those results. |
| **G15 — Storage stalls** | Scoped menu improvement: backup/verify/restore runs on one background worker and exposes busy/error state. Main authority storage/clone/install performance remains open. | Measure end-to-end pause and transition distributions, including clone, complete validation, install and final close; preserve mutation barriers and acknowledgements after durability. Responsive menu polling does not prove short authority storage stalls or a cancellable filesystem operation. |
| **G16 — Operational recovery** | Scoped improvement: supported saved managed worlds can create private complete wrappers, review exact checkpoint/profile metadata and restore into a new managed world after original-tree loss. Actual native reopen and result-reachability acceptance still attach separately. Deployment recovery remains open **conditional P1**. | Rehearse matching-version restore after original loss and corrupted/stale/partial inputs, preserving complete private/world/profile/source state and all old copies. Set retention/confidentiality/offsite transfer/owners/RPO/RTO and rehearse on the deployment platform. Define explicit supported policies for portable JSON, historical/live/custom catalogs and servers rather than implying current local bundles cover them. |
| **G17 — Observability** | Existing retained periodic diagnostics unchanged; native recovery gains named operation/result/errors. Crash/operational observability remains open. | Establish crash diagnostics, durable external audit/retention, actionable alert thresholds, ownership and tested failure detection. A surviving sample, a missing final export or a busy indicator is not a health or availability certificate. |
| **G18 — Global architecture** | Unchanged conditional expansion; regional paging and local cluster routing remain single-authority techniques. | Establish real player/data/geographic requirements first; if justified, design authority partitions, one-writer ownership, cross-shard economy/transport handoff, fault recovery and adaptive delivery with conservation and privacy proofs. |

## Exact claim boundaries for the new evidence

### Navigation

The focused authored three-parked-body case reached its unchanged target in **14.5 physical seconds / 145 steps of 0.1 seconds**, ending **0.26763 m** from the goal. It travelled **75.48866 m** with maximum lateral excursion **18.65695 m**, maximum admitted step **0.800000002 m**, maximum heading change **3.36781°**, and at most **3** saved path points in that case. It recorded **10,512** source samples, a maximum **5,760** per update against the fixture's **16,384** ceiling, and **143** exact resumed continuation steps. The general fallback bounds are four encountered stopped bodies, 12 additional corridor attempts per vehicle update, 64 m corner reach, 160 m complete proposal length and the existing 129-waypoint cap.

The original source baseline advanced only **7.302238 m** north in 120 physical seconds on unbounded flat support. The final progress fixture additionally bounds the supported flat region. State that difference; this is not a timed hardware comparison. Original queue fixtures remain independent unchanged evidence (rear-trap 34.0 s; occupied-exit 49.5 s). The navigation report is not proof of native rendered traffic throughput, company profitability, prepared-world coverage, collision impulses or general shortest-path planning.

### Presentation

The optional direct demo manifest has six models; the unchanged default woodland pack has five. The Comet inspection records 91 nodes, 2,276 triangles and two named clips, `parked` and `rolling`. Runtime rig rotation is not limited to the standalone preview clip's playback speed: signed admitted motion and steering drive bounded presentation only. Retain exact direct and recipe-built identities from their separate reports. These counts are asset admission data, not GPU benchmarks or a completed production art library.

### Recovery

The launcher wraps the existing supported local bundle; it does not expand its host/source/catalog scope. The bounded report may describe counts and pass/fail assertions but should not export the player's private world or capability data. Complete archive equality, metadata/source fidelity, original-copy preservation, unpublished failures and actual native reopening are different checks and should all be named honestly. Unix 0700/0600 and hard-link rejection do not by themselves certify Windows ACL or removable-volume privacy.

## Finalization checklist for the report author

- Reconcile source/version/compatibility, final executed gates, accepted binary and native evidence before claiming release acceptance.
- Record the independent P2 result-pin finding, owner's fix and regression outcome.
- Include the three current-guide corrections above in the documentation polish record.
- Retain all G01–G18 rows; distinguish scoped improvement from original closure.
- Keep baseline archives, fixture identities and old measurement values unchanged.
- Ensure the 0.15 report and its evidence are in the final source ZIP and current links resolve.

## Audit follow-up before handoff

The independent launcher's sole P2 finding is now corrected and re-reviewed. Exact session-level completed-result pins survive the bounded browser rescans, navigation and refresh. The owner retained the 24-entry limits and stale-identity guards; failed rereads no longer produce an unconditional success claim. The actual poll/action regression covers 24 future-dated worlds and backups. I inspected `/tmp/terra-build/launcher-focused-015.log`: 23 launcher tests passed, zero failed/ignored, including that regression. No Cargo rerun was performed by this audit agent. This supersedes the pending-owner wording above for the completed-result issue; unsupported-workflow help wording and final native evidence remain separate follow-ups.
