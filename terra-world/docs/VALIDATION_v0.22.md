# Validation — Terra World 0.22.0

Executed on Linux x86_64 using the pinned
`rustc 1.99.1 (58cf8f9c3 2026-10-13)` toolchain and Python 3.12.14.
The review began on 29 October 2026. Receipt timestamps use explicit UTC.

## Direct final acceptance

**PASS — all 44 canonical gates completed successfully.** The runner exited
zero and published its [terminal receipt](verification/v0.22-release-acceptance/completed-gates.json)
with **1,043 distinct passing Rust tests** across
**1,144 passing Rust test executions**, plus
**46 Python tests** in the tooling gate. Every gate exited zero; there were no
failed or ignored Rust tests, Python skips, or gate timeouts. Candidate and
interrupted attempts are excluded from these totals.

Command, run from the project root with the pinned toolchain and target directory:

```sh
python3 scripts/verify.py --offline --jobs 3 --report-dir docs/verification/v0.22-release-acceptance
```

Started **2026-10-29T22:04:10.701022+00:00**; finished
**2026-10-29T22:15:38.769213+00:00** — **688.068 seconds** overall.
The terminal record and `gates.json` progress alias are byte-identical. No receipt
was reconstructed. All **378 frozen implementation inputs**,
**36 published evidence artifacts** and **44 gate logs**
retain their accepted hashes. Documentation records the results afterward;
the final archive inventory binds those delivered document bytes separately.

Terminal receipt SHA-256:
`7992f2fb0821faaa1f72d96f0d5b69f0fea18cd3acbc593214109dc1db893085`.

Accepted canonical source-map identity:
`261b8840abe316cb873cb9561e52115b453fe4826a430a609b20166351cf4510`.

The [distinct-test inventory](verification/v0.22-release-acceptance/distinct-tests.json)
counts each crate/harness/test identity once. Repeated targeted or feature-mode
executions remain visible in the larger execution total:

| Workspace crate | Distinct passing Rust tests |
|---|---:|
| `terra-bevy` | 180 |
| `terra-cli` | 4 |
| `terra-core` | 33 |
| `terra-demo` | 227 |
| `terra-history` | 13 |
| `terra-net` | 128 |
| `terra-persistence` | 49 |
| `terra-sandbox` | 364 |
| `terra-stream` | 45 |
| **Total** | **1,043** |

The [regional TLS workload](verification/v0.22-release-acceptance/regional-sessions.json)
completed with 16 real socket clients, 8 churn cycles, 4,840 fixed steps and 1,024
cold overlay regions. Accepted checks include offline/resumed/restarted ownership,
source-unavailable blocking and restored movement, failed-storage retry, conserved
cargo, exact command sequences and authenticated backup recovery. This is a finite
local correctness workload; its timings are not a WAN or population-capacity claim.

The native-build gate succeeded. Its exact executable is byte-identical to the
one used for the final native walkthrough; the separately generated
[native association record](verification/v0.22-native/confirmation-input-association.json)
binds both source maps through the sole two-byte test-only correction described
below.

## Executed focused coverage

The selected regressions exercise actual command, adapter, filesystem, process
and UI-system paths. They do not merely reproduce the implementation in a test.
The raw candidate logs remain under `verification/v0.22-preflight`.

| Focused preflight | Result and scope |
|---|---|
| New dissolution plus existing faction lifecycle | 13 passed: 5 new dissolution cases and 8 retained lifecycle cases; no failed or ignored tests. |
| Authenticated TLS faction lifecycle | 2 passed, including new dissolution, privacy, sequence/replay, host save/restart and resumed admission. |
| Bevy library with TLS | 140 passed, including actual dormant-observer behavior, uncertain Welcome identity publication and nested recovery marker faults. |
| SQLite uncertain backup | 1 passed: complete retained checkpoint reopens and verifies, original state is conserved and a no-clobber retry refuses replacement. |
| Initial shared publication | 6 passed. A seventh ancestor-directory regression was added afterward and is included in final canonical scope. |
| Integrated demo before native polish | 226 passed, zero failed or ignored, covering actor summaries/labels/actions, managed leases and new-world/save/recovery failures. |
| Integrated demo after native polish | **227 passed**, zero failed or ignored. The new F6 capture/navigation/activation regression verifies Cancel-first entry and delayed readiness. |
| Corrected Python tooling | 46 passed, zero skips. The fresh canonical Python gate independently ran the same 46 cases. |

The canonical roster also covers existing domain economy/ownership, actual
TCP/TLS, regional transactions and restoration, purchased-stock and one-hour
novice passenger-career scenarios, physical traffic/navigation, ongoing guidance,
headless/render adapters, managed recovery, provider examples, original asset
inspection/generation/authoring, prepared sources, recovery probes, a finite
16-client TLS regional-session workload and the exact native executable build.
Its actual results and counts belong to the terminal receipt above.

### Selected 0.22 behavior established by the regressions

- **Actor context and targeting:** empty/missing/foreign/group/vehicle/remote
  states; group minima limited to selected owned biological bodies; exact
  admitted driver identity; actual labels/bars/action availability; native
  displayed player/restore/body binding; queued body-transition rejection; and
  explicit owned-ID retention for Stop/Rest.
- **Community lifecycle:** active sole-Leader eligibility; rejection for unknown,
  dormant, outsider, invitee, multi-member and asset-owning cases; protection of
  far land/territory and cold shared property; invitation revocation; no refunds,
  asset movement or ID reuse; stored continuity and real network replay/restart.
  Exact displayed review tokens reject stale and hidden confirmation. Actual
  focus navigation preserves cancellation, deliberate confirmation and expiry.
- **Ecology:** the real native `advance_population` adapter distinguishes dormant,
  resumed and dormant-again owners without moving bodies or resetting the clock.
- **Publication and recovery:** actual file/directory sync faults before and after
  namespace publication, replace/no-clobber intent, relative basenames, exclusive
  private staging under permissive umask, checked new ancestors, readable retained
  outputs, completed-tree/final-marker boundaries, and nested native wrappers.
  Actual native final-save uncertainty returns an error exit and retains dirty
  state; actual Welcome uncertainty retains private identity but refuses admission.
- **Managed lifetime:** real separate-process contention, independent worlds,
  clean and forced holder release, ownership through final metadata saving,
  immutable witness continuity and reserved profile/witness/lease output aliases.
- **Desktop inspection:** bounded exact inventory, portable namespaces and alias
  refusal, symlink/reparse/hardlink checks, byte and architecture corruption,
  receipt/evidence association, strict counters, passive non-execution and
  explicit bounded smoke success/failure/timeout behavior.

## Candidate faults and corrections retained

The first coordinated demo compilation failed because a new test used
`assert_eq!` with `Panel`, which does not implement `Debug`. The owner corrected
the assertion; no application behavior was changed for that fixture. The same
candidate reported a 1.99 deprecation warning for `AtomicU64::fetch_update`.
The checked monotonic token allocator now uses a compare-exchange loop compatible
with the minimum Rust toolchain. Raw failed output remains in `demo.log`; the
later complete demo runs passed. The now-unused RestSelected presentation intent
was removed after native Rest moved to direct owned-ID snapshot dispatch.

Independent cross-review closed a queued community confirmation that could target
its replacement review, hidden review binding, unchecked newly created ancestors,
inner-versus-wrapper recovery completion, hard-linked desktop payloads, an
unbounded receipt alias, and permissive failed/ignored counter types. Those
findings are recorded with their scoped fixes in the current review.

Actual native inspection then found two remaining presentation/consent issues:
an inspect-only selection still received a generic possession hint, and F6
management entry selected Confirm despite a Cancel-first layout. One correction
batch aligned the toast with actual primary ownership and changed the real focus
handoff to Cancel with no transient Confirm fallback. Singular member/invitation
copy was also corrected. The rebuilt 227-test suite and final native confirmation
below passed. Candidate screenshots, including the demonstrated defects, remain.

The first canonical attempt, retained at `verification/v0.22-final-acceptance`,
finished **failed** on the headless gate. An existing privacy/wire-format test
still expected protocol 10 in its Hello and Welcome JSON, while the intended
implementation emitted 11. Its preceding redaction checks passed. Only those
two literals were changed, both inside `#[cfg(test)]`; the focused regression
then passed. Independent [source comparison](verification/v0.22-review/test-only-source-review.md)
proves exactly two changed bytes, an identical 12,286-byte production prefix,
and no removed/weakened assertions. The failed receipt/logs remain unchanged and
are excluded from accepted totals. The fresh run uses a different directory.

A second attempt, retained unchanged at `verification/v0.22-confirmed-acceptance`,
was interrupted without a terminal receipt. Its progress file records **42**
completed zero-exit gates and a running regional-session gate. The workload's
sidecar was written, but neither that sidecar nor partial progress qualifies
release acceptance. Scoped process inspection found no surviving runner/workload;
the cause was not captured. All **378** implementation inputs still matched.
The [interruption record](verification/v0.22-review/acceptance-interruption.json)
binds the retained progress, source map and outer log. No receipt was reconstructed
and no implementation correction was inferred. All 44 gates were rerun in the
fresh `v0.22-release-acceptance` directory using the existing build cache; that
completed run is the accepted result above.

The [independent diagnostic](verification/v0.22-review/interruption-diagnostic.md)
confirmed that the verifier and regional workload source are byte-identical to
the delivered 0.21 archive and that the observed timeline precedes the configured
gate deadline. It found no substantiated cause. The sidecar establishes that its
write was reached; it does not establish child exit or runner acceptance.

## Fresh native inspection and final confirmation

The [native record](verification/v0.22-native/README.md) contains the complete
method, fresh 0.21 baseline, 0.22 candidate, correction batch and direct final
confirmation. The application was operated on Linux X11/Vulkan llvmpipe software
rendering at **1440×900** and **1100×700 with 150% text**. Captures contain actual
native pixels, per-image hashes/dimensions and window PID checks before/after.

The candidate created a separate managed regional world through character,
world settings and the named review, then exercised Activities, group/empty/
inspect-only selection, Command scrolling, first/third-person possession,
vehicle selection and driver needs, named community creation, cancellation,
dissolution, pause, keyboard reachability and Save and quit. The initial pending
terrain frame was explicitly Loading; later settled frames rendered the world.
It is not a first-frame timing or hardware-performance measurement.

The direct final process **212865** reopened that exact managed world, visibly
confirmed the ownership-toast correction and retained prior dissolution, created
fresh community #2, entered review on Cancel, canceled through Enter, then
opened a separate review and deliberately navigated to Confirm before applying
it. Save and quit completed with exit **0** at **21:33:39 UTC**.

Final native executable SHA-256:
`34dc6fe5c906d556fcf182968df0a92742079008c5eb26cc94d652a69a041cd6`.

All **378** inputs remained unchanged during that direct process, with canonical
source-map identity:
`ec3fb5123a82cd1d4e6f5351d8f04503d305359628a44f61d8e032d9eca5902d`.

Read-only closed-world evidence reports SQLite integrity **ok**, checkpoint
presence, generation **60** versus candidate generation **40**, no factions and
next domain ID **3** after removal of IDs 1 and 2. Wallet, player inventory,
launch-witness hash and lease-marker hash remained unchanged. Profile, witness,
lease and database retain mode 0600. These are observed saved values from this
isolated journey; wider conservation and failure cases are independently tested.

The player specialist independently inspected the final correction/focus/save
images and verified their hashes, dimensions and PID association. The world
specialist independently checked the lifecycle/focus captures and saved-state
comparison as well. The later test-only correction changes one file in the
378-input map; its [independent review](verification/v0.22-review/test-only-source-review.md)
retains the exact native-confirmed source for comparison. The accepted native
rebuild matches the exact executable hash above. The
[executed association check](verification/v0.22-native/confirmation-input-association.json)
confirms the sole changed file, exactly two test-only bytes, identical production
prefix and all other source inputs, and equal native/accepted/current executable
hashes. The accepted source-map identity is
`261b8840abe316cb873cb9561e52115b453fe4826a430a609b20166351cf4510`;
it remains explicitly distinct from the native observation's earlier test map.
No runtime edit followed the captured confirmation.

## Review, documentation and preservation

Three dedicated specialists and the integrator reviewed product fit, entry and
return journeys, interaction, accessibility, progression, economy, communities,
ecological authority, vehicles/navigation, geography/provenance, rendering,
SDK/API integration, networking, security/privacy, persistence, concurrency,
operations and delivery. Findings, research, dispatched ownership, meaningful
acceptance conditions and cross-review are retained in the
[plan](IMPLEMENTATION_PLAN_v0.22.md), [research](RESEARCH_v0.22.md),
[current review](PRODUCT_REVIEW.md) and [G01–G18 register](REVIEW_GAPS_v0.22.md).

The prepackaging [preservation audit](verification/v0.22-review/preservation.md)
independently checked all **2,376** files in the exact delivered 0.21 archive.
It found no missing or unexplained changes. All protected assets, source data,
configuration, authoring inputs, licenses, historical screenshots and evidence
were byte-identical. Its explicit allowlist contains every permitted current
source/document edit; native-polish additions are recorded as later scope
amendments without rewriting the earlier observed hashes.

The exact prior current review and validation remain as
`PRODUCT_REVIEW_v0.21.md` and `VALIDATION_v0.21.md`. The prior generated delivery
manifest is retained at
`verification/v0.22-review/baseline-delivery-manifest.json`. Final source packaging
must re-read every baseline member through that explicit one-path relocation,
restore original ZIP Unix modes, bind accepted evidence/native identity, and
verify every new archive member. The independent source-packager review is
separate from the actual packaging result.

## Artifact verification

The complete source handoff's [delivery record](verification/v0.22-delivery/README.md)
records archive creation, clean desktop extraction, passive inspection and the
finite explicit `--help` smoke result. The named JSON records supply exact
archive, extracted inventory, accepted executable and receipt identities. These
artifact checks are separate from the canonical tests and native window journey.

The paired `terra-world-0.22-source-verification.json` is emitted outside the
source archive after its final byte/mode readback. It reports the baseline-member
mapping, intentional edits, exact historical copies, restored original Unix
modes and accepted-source/evidence/binary association. The archive's generated
`DELIVERY_MANIFEST.json` binds every included source file and excludes itself.
Use those actual records for final artifact outcomes; a package filename or a
review of its tooling alone does not establish successful delivery.

## Compatibility and remaining qualification

Release **0.22.0**, wire **11**, TLS **terra-world/11**: upgrade clients and hosts
together. Domain save, regional database and asset schemas remain **1**. All nine
workspace packages are aligned; external dependency identities are unchanged.
Rust minimum remains **1.97.1**, acceptance compiler **1.99.1**. New public
command/error variants require downstream exhaustive matches to be updated.
Source/catalog identities remain mandatory; no content or source migration is
introduced by matching save-format numbers.

The managed witness records admitted initialization, not successful saving.
The stable lease records the participating process's handle lifetime, not stale
PID ownership; do not unlink it to force another session. Older binaries, direct
SDK writers, arbitrary administrative path replacement and network filesystems
are outside the cooperative guarantee. Visible but unconfirmed publication is
retained and reported, never silently acknowledged or claimed rolled back.
A retained completed backend does not invent a missing native recovery wrapper.

The desktop is a Linux x86_64 optimized **dev-profile review build**, not a
qualified Windows installation or release-profile benchmark. Actual Windows
clean-machine execution remains **G02/P1** for the intended PC. Physical GPU,
controller/wheel, assistive, localization and broader scaling qualification are
not supplied by software rendering or logical event regressions. Unix modes do
not establish Windows ACLs; injected errors and clean reopen do not prove
physical power-loss survival.

Uncoached sustained careers, populated-community policy, dense/imported
navigation, production art/audio/licensed data, content migration, authenticated
updates, public accounts/moderation, WAN/soaks, storage tail budgets and offsite
RPO/RTO retain their explicit next proof in the gap register. Local hashes and
readback establish consistency, not publisher authentication. No whole-product
AAA/MMORPG readiness claim is inferred from this increment.
