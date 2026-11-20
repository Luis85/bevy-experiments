# Terra World 0.23 — improvement and polishing plan

## Objective and baseline

Deliver the next complete framework increment from the exact delivered 0.22.0
source, reviewed on 30 October 2026. Preserve the native forest-green interface,
the finite economy, authoritative simulation and source identities. Improve
the full path from a displayed action through execution, save, maintenance and
desktop handoff. This plan precedes implementation dispatch.

Baseline: `terra-world-0.22.0.zip`, SHA-256
`54ba5420db937056ba62e1f7861308a4ea540112f52ae500af10e92e897a739e`,
2,790 entries. Its archived evidence remains historical; all 0.23 acceptance is
new. Reviewers independently studied product/design docs, G01–G18, source and
primary guidance. Native visual findings require current screenshots.

## Review and ownership

| Owner | Product perspectives | Implementation boundary |
|---|---|---|
| Player specialist | First/returning session, comprehension, actions, accessibility, controls, layout, feedback and consent | Native frontier/interface/interaction and actor-summary integration |
| World/framework specialist | Goals, progression, resources, finite economy, communities, simulation, multiplayer authority and reusable SDK | Domain settlement/readiness, guidance and meaningful integration fixtures |
| Reliability specialist | Progress safety, maintenance concurrency, recovery, privacy, desktop verification and platform qualification | Launcher lease/recovery, standalone verifier and their tests |
| Integrator | Product scope, research synthesis, preservation, compatibility, native evidence, release gates and final delivery | Version/docs, toolchain, integration, native journeys, acceptance and packaging |

No shared file is edited concurrently without an explicit ownership handoff.
Cargo jobs are coordinated centrally. Each specialist reviews a peer's changes
after implementing their package. Review reports distinguish observed behavior,
source findings, design decisions and qualification still requiring a device or
user study.

## Prioritized findings

1. Native Nature and direct Activities consumption do not retain the same
   displayed body context already used by Command. Queued possession/restore
   changes can consume for another body.
2. Resource and territory helpers can replace a missing explicit target with a
   nearest/first target. Side-effectful queued actions can therefore act on a
   destination the player did not review.
3. Supply guidance advertises completion from goods/reach alone, while authority
   can reject receiver or wallet capacity. Expired unclaimed offers can remain
   advertised as acceptable until maintenance. Blockers need concrete recovery.
4. Acquisition guidance does not consistently account for complete inventory
   exchange capacity before proposing gather/craft actions.
5. Launcher rename and native backup do not participate in the managed-world
   lease. Rename can race final session metadata; backup can observe changing
   native metadata and produce an avoidable incomplete wrapper.
6. Desktop startup smoke output is limited only after the process exits. An
   unlimited temporary spool can consume disk during the supposedly bounded check.

## Implementation packages and acceptance

### WP01 — consistent native consumption consent

**Player specialist.** Bind Command, Nature and Activities consumption to the
explicitly named active body shown to the player. Preserve player, restore and
body context through label, activation, queue and dispatch. Reject intervening
possession/release/restore/network transitions, without mutating inventory or
needs. Use one shared contract; do not add cross-session wire reservations.

Acceptance: actual activation and dispatch fixtures prove unchanged-context
success and changed-body/player/restore/pending-transition rejection. The label
names the real person even when another actor is selected or inspected.

### WP02 — retain resource and territory action targets

**Player specialist.** Side-effectful frontier actions retain the displayed
resource/territory and relevant context. Missing explicit selection is an
unavailable state, never implicit permission to choose a different object.
Travel/gather/contribution/fund/upgrade retain their exact target; domain
authority still rechecks reach, rights, capacity and value at execution.

Acceptance: changed selection, disappeared/replaced destination and queued
chooser/action sequences reject safely; unchanged context executes normally.
No goods, money, ownership or position changes on rejection. Target names and
written recovery instructions remain reachable in the existing scroll layout.

### WP03 — shared supply readiness and actionable recovery

**World/framework specialist; player specialist integrates native UI.** Add a
read-only typed readiness/preparation seam used by authoritative fulfillment,
guidance and the native noticeboard. Explain actual blockers: ownership/expiry,
missing or out-of-view destination, reach, carried goods, receiver capacity and
reward-wallet capacity. Readiness is a current-condition check, not a reservation.
Do not advertise an expired offer as Accept. Preserve exact offer identity.

Acceptance: every blocker preserves goods, escrow and wallet; restored/partial
views are explicit; full receiver or wallet can recover and then complete; two
competing final-capacity deliveries cannot overfill; exact funded reward reaches
the rightful owner. Actual native label/disabled-state and command use agree.

### WP04 — achievable acquisition guidance and continued journeys

**World/framework specialist.** Reuse domain inventory exchange rules to avoid
recommending impossible gather/craft steps. Account for consumed inputs freeing
space and all outputs occupying it, with bounded work and no second inventory
ledger. Prefer an executable existing alternative or explain what must change.

Acceptance: full/near-full stacks, item slots, multi-output custom recipes,
insufficient inputs and successful alternatives. Finite integration fixtures
exercise supply acquisition, delivery, failure recovery and save/restore
continuation with conserved value. Existing Home/Community and Road/Company
contracts retain their authority and regression coverage. Automated journeys
are not described as uncoached human playtests.

### WP05 — coordinated managed metadata maintenance

**Reliability specialist.** Introduce a metadata-only acquisition path for the
existing stable cooperative world lease. Rename acquires it before profile
validation/read and holds it through publication. It must not create a launch
witness, initialize progress or imply a saved checkpoint.

Acceptance: active-session and concurrent-maintenance contention refuse before
mutation; bytes remain unchanged; rename succeeds after release; never-started
worlds retain their initialization state; held lease survives through final write.
Document cooperative/older-writer and platform boundaries precisely.

### WP06 — coherent native backup maintenance

**Reliability specialist.** Native managed backup uses the same maintenance
lease to snapshot profile and progress coherently; an active world is refused
before destination creation. Keep the backend's intentional online SQLite backup
API and existing completed-artifact retention rules.

Acceptance: active-world backup cannot leave output or alter the original;
quiescent supported regional backups verify and recover into a new world; JSON
managed profiles retain rename/lease safety and the existing manual archival
policy rather than gaining an unplanned backup format. Leases
release on all failure paths; completed inner artifacts remain retained when
later publication fails. Named next action tells the player to finish the session.

### WP07 — bounded desktop startup verification

**Reliability specialist.** Replace unbounded temporary output spooling with
bounded streaming capture, finite deadline, overflow detection and child-tree
cleanup. Preserve passive verification's nonexecution contract and JSON output.
Portable behavior and limitations must be explicit.

Acceptance: real child fixtures cover normal help, failure, output overflow,
timeout and descendant-held pipes. A broken process cannot grow a disk spool
without bound, hang the reader or leave an accepted success. Keep existing
manifest/path/hash/receipt checks and source-free operation intact.

### WP08 — release integration and researched documentation

**Integrator.** Advance workspace packages to 0.23.0; preserve pinned external
dependencies and saved schemas. No planned command serialization change means
wire 11/ALPN terra-world/11 remain unless implementation demonstrates necessity.
Document public readiness API additions and exact compatibility. Archive exact
prior current review/validation and root delivery manifest before replacement.
Maintain all G01–G18 with scoped closures and explicit next proof.

Acceptance: build/test/version contracts agree; current review/research/plan,
validation and affected guides cross-link; no broken local references; all
baseline content is preserved or an intentional edit is individually recorded.

### WP09 — independent review and bounded native polish

**All specialists, coordinated by integrator.** Rotate cross-reviews across
action context, economic readiness and persistence/process boundaries. Capture
a current baseline, then complete candidate journeys at 1440×900 and
1100×700/150% text: create/continue, Activities, Nature, territory, blocked and
successful supply, pause/save/reopen, rename and backup/recovery where supported.
Inspect every accepted image. Fix material findings in one consolidated batch,
then confirm once. Do not spend the pass on optional cosmetic micro-iterations.

Acceptance: actual native pixels and functional outcomes, visible focus and
reachable primary/recovery controls, captured process/source/binary association,
and isolated test worlds. Software Linux evidence never becomes a Windows,
physical-GPU/controller/assistive or human-study qualification claim.

### WP10 — canonical acceptance and complete handoff

**Integrator with independent specialist verification.** Freeze implementation
inputs; run the complete canonical gate roster, including new meaningful tests.
Retain actual failed attempts separately. Only a fresh successful terminal
receipt with exact source/log/report/binary identities authorizes packaging.
Deliver complete source and accepted Linux desktop archives, review/research,
this plan, current gaps, validation and source verification.

Acceptance: independent fresh extraction passes passive and explicit bounded
smoke checks; payload remains unchanged; ZIP bytes/modes and complete baseline
preservation are verified; native evidence binds the accepted binary; all
deliverables are saved and linked. Windows remains a separate executable gate.

## Sequence and scope control

1. Verify exact baseline; study docs and primary research; collect review and
   fresh native evidence. Preserve historical current documents.
2. Dispatch WP01–02 and native WP03 integration to player; WP03–04 to world;
   WP05–07 to reliability. Integrator carries WP08 and runtime/native work.
3. Run coordinated focused checks, rotate peer reviews, resolve findings.
4. Finish WP09 candidate/correction/confirmation and freeze implementation.
5. Execute WP10 canonical gates, finish docs, package, independently verify and
   deliver. An implementation change after freeze requires fresh appropriate
   acceptance; no receipt reconstruction or relabeling.

New material defects may expand a package with written rationale. Disproved
hypotheses are recorded and removed. Avoid new currencies, invented refunds,
asset confiscation, speculative distributed authority or dependency churn.

## Explicit next-product boundaries

Actual Windows/physical-device proof remains P1 for the intended PC. Uncoached
long careers, populated-community succession/dissolution, sustainable treasury
policy, physical GPU and storage tail budgets, production art/audio, licensed
regional ingestion, migration, signed updates, Internet account/moderation,
WAN/soak capacity and offsite RPO/RTO retain their own acceptance criteria.
This increment closes specific workflow defects without declaring those broader
product requirements complete.

## Execution status

The plan was written before implementation and executed through the three named
specialist boundaries. The following ledger records actual completed work;
final canonical and artifact identities are supplied by Validation and the
delivery records rather than predicted here.

| Package | Implemented increment and executed evidence |
|---|---|
| WP01 | Shared displayed-person intent across Command, Nature and Activities; real Activate-to-dispatch tests reject changed ownership/body/player/restore and pending transitions without mutation. |
| WP02 | Exact resource and territory context retained; missing explicit IDs stay unavailable; chooser/action and target replacement regressions exercise dispatch. |
| WP03 | Additive typed Accept/Deliver/Return readiness shares command preparation; native labels, disabled state and pre-enqueue checks use it; capacity, ownership, expiry and receiver-view recovery are tested. |
| WP04 | Full inventory exchanges account for all outputs and freed input slots, bounded feasible batches and real alternatives. Dormant owners receive no executable guidance. Seventy focused tests passed across five domain suites. |
| WP05 | Metadata-only managed lease holds through authoritative reread and rename publication. Real child processes cover busy, normal and abrupt release, first-launch state and publication boundaries. |
| WP06 | Regional native backup refuses busy ownership before output allocation, holds ownership through wrapper publication and recovers a complete separate world. JSON retains its documented manual-archive scope. Native checkpoint 23 recovery was executed. |
| WP07 | Streaming output is capped at 65,537 bytes including the overflow byte; deadline includes pipe EOF; POSIX surviving groups prevent success. Fifty actual Python fixtures passed before canonical rerun. |
| WP08 | Workspace 0.23, exact unchanged external pins, wire 11/ALPN 11 and schemas 1 documented. Prior current reports/manifests retained exactly; all 18 broader gaps carried forward with next proof. |
| WP09 | Fresh 0.22 baseline, 52 candidate images, one consolidated copy batch, and ten final direct-PID confirmation images. Both final processes exited0 with identical unchanged 381-input digest; recovered money/progression persisted. |
| WP10 | All 44 canonical gates passed: 1,070 distinct Rust tests, 1,171 executions and 50 Python tests, with unchanged implementation/evidence and the exact native-confirmed executable. Final source/desktop artifact checks are recorded separately in the delivery record. |

### Material review corrections

Peer review corrected inactive-owner guidance on early branches and a desktop
help process that could leave a redirected background child. Integrator review
replaced a deprecated token allocator while preserving the Rust minimum. The
scope was clarified before implementation to avoid promising a native JSON
backup format. These corrections strengthened the selected contracts without
inventing new economic, save or transport behavior.

The single native correction batch clarified effective Performance sky state
while preserving its stored preference, used singular completed-contract copy,
and displayed readable item names. Screenshot-driven authorization added
`launcher/ui.rs` and `flow.rs` to the explicit preservation policy. Final source
was formatted and rebuilt before confirmation; all further work is evidence,
documentation and packaging.

### Evidence coverage and limits

The native journey completed accept/travel/deliver, then found/fund/save/rename/
backup/verify/recover. Travel guidance represented out-of-range delivery; the
delivery action appeared on arrival. Nature visibly explained reach and missing
consumables. Full receiver/wallet-capacity, expired/disappeared target and active
maintenance refusal are covered by executable adversarial tests rather than
staged GUI snapshots. Reopening was confirmed with a direct final game process.

The candidate child window had a stable observed PID but no independent child
process receipt. Final direct game and menu receipts close that provenance gap;
their association with the actual canonical receipt is checked separately.
Original and backup have complete logical equality at checkpoint 23. Recovered
simulation advances normally, preserving measured progression/treasury values;
whole-world equality after resumed play is not asserted.

All G01–G18 remain tracked. Selected action/readiness/maintenance/startup defects
are resolved; actual Windows/device qualification, uncoached careers, physical
budgets, populated-community policy and production deployment/content retain
their explicit next-increment criteria.
