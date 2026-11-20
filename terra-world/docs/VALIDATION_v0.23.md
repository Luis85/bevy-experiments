# Validation — Terra World 0.23.0

Executed on Linux x86_64 with the pinned Rust 1.99.1 toolchain. Review and
execution date: **30 October 2026**. Machine receipt timestamps use explicit UTC.
The exact prior report remains at [VALIDATION_v0.22.md](VALIDATION_v0.22.md).

## Canonical acceptance

**PASS — all 44 canonical gates completed successfully.** The runner
exited zero and published its [terminal receipt](verification/v0.23-release-acceptance/completed-gates.json)
with **1,070 distinct passing Rust tests** across
**1,171 passing Rust test executions**, plus
**50 Python tests**. Every gate exited zero; there were no failed or ignored
Rust tests, Python skips, or gate timeouts. Candidate/preflight executions are
excluded from these totals.

Started **2026-10-30T08:34:46.754780+00:00** and finished **2026-10-30T09:05:55.903708+00:00**,
**1,869.149 seconds** overall. This cold-environment run included compilation
of the test configurations; it is not a runtime performance benchmark. Python
was **3.12.14**, with three Cargo build jobs and offline
locked dependencies. The terminal receipt and progress alias are byte-identical.
All **381 implementation inputs**, **36 published evidence artifacts**
and **44 gate logs** retain their accepted hashes.

Terminal receipt SHA-256:
`12b4a3f72591c7431bbc0aeda6fe189c5650b81da29d29c7fa8ef955c903cfa0`.

Accepted source-map identity:
`dd6d40911a1defed04d39a3a4287ae01ced1cfde27fdcbf0575caa404c39c5d5`.

The [distinct-test inventory](verification/v0.23-release-acceptance/distinct-tests.json)
counts each crate/harness/test identity once; repeated targeted and feature-mode
executions remain visible in the larger execution total.

| Workspace crate | Distinct passing Rust tests |
|---|---:|
| `terra-bevy` | 180 |
| `terra-cli` | 4 |
| `terra-core` | 33 |
| `terra-demo` | 238 |
| `terra-history` | 13 |
| `terra-net` | 128 |
| `terra-persistence` | 49 |
| `terra-sandbox` | 380 |
| `terra-stream` | 45 |
| **Total** | **1,070** |

The [regional TLS workload](verification/v0.23-release-acceptance/regional-sessions.json)
completed with **16 real socket clients**, **8 churn cycles**, **4,840 fixed
steps** and **1,024 cold overlay regions**. Accepted assertions cover offline,
resumed and restarted ownership, source-unavailable blocking/recovery,
failed-storage retry, conserved cargo, exact inventory/command sequences and
private authenticated backup recovery. This finite local workload does not
establish WAN or population capacity.

Command, with the pinned toolchain and isolated target directory:

```sh
python3 scripts/verify.py --offline --jobs 3 --report-dir docs/verification/v0.23-release-acceptance
```

The unchanged roster covers compatibility, Python packaging tools, formatting,
all headless crates, career reports, traffic/navigation and guidance continuity,
Bevy headless/render adapters, native demo and launcher recovery, provider
examples, original asset inspection/generation/authoring, prepared sources,
recovery probes, finite local TCP/TLS workloads and the native executable build.
A local correctness workload does not establish WAN or deployment capacity.

## Executed focused coverage

These are actual candidate executions retained separately from final acceptance.
Their totals are not added to canonical distinct-test counts.

| Focused execution | Result | Evidence |
|---|---:|---|
| Supply readiness, inventory capacity, existing guidance/commitments and settlements | **70 passed**, 0 failed/ignored, five suites | [Domain receipt](verification/v0.23-preflight/sandbox-integration.json) and [log](verification/v0.23-preflight/sandbox-integration.log) |
| Complete native demo test binary before the final copy-only batch | **238 passed**, 0 failed/ignored | [Native receipt](verification/v0.23-preflight/native-integration.json) and [log](verification/v0.23-preflight/native-integration.log) |
| Corrected desktop tooling with real child-process fixtures | **50 passed**, no skips/failures | [Python receipt](verification/v0.23-review/reliability-python-polish-tests.json) and [log](verification/v0.23-review/reliability-python-polish-tests.log) |
| Native candidate build | Exit 0 | [Candidate build](verification/v0.23-preflight/candidate-build.json) |
| Final copy-correction native build | Exit 0, 58.329 seconds | [Confirmation build](verification/v0.23-preflight/confirmation-build.json) |

### Intent and native action boundaries

Actual Bevy activation, action queues and dispatch exercise the displayed active
person across Command, Nature and Activities. Unchanged context can execute;
changed body, player, owner, restore revision, dormant/disconnected state or a
pending body transition reject before mutation. Resource/territory selection
changes and missing explicit IDs do not silently retarget an action. The tests
check conserved inventory, needs, wallet, movement and ownership, not just copied
predicate values.

The shared supply query drives native labels, disabled actions and pre-enqueue
checks. Authority repeats its own preparation at execution. Context tokens are
local reviewed-intent protection; they are neither remote reservations nor a
replacement for the existing command sequence/replay protocol.

### Domain readiness, capacity and continuation

The new public readiness types cover Accept, Deliver and Return using the same
preparation as authority. Tests cover owner/account/dormancy, current expiry,
missing or out-of-view receiving sites, position/reach, goods, receiver inventory
and reward-wallet capacity. Rejection preserves finite goods, escrow and money;
recovery allows completion. Competing deliveries cannot both consume the last
capacity admitted by an earlier read.

Acquisition tests account for consumed inputs freeing slots, every output of
multi-output recipes, stack limits, smaller bounded craft/gather batches and
available alternatives. Ready-looking guidance is suppressed for inactive
owners across all six tracks. Existing disk save/restore and commitments tests
exercise finite continuation without inventing goods or issuer funds.

### Managed maintenance and recovery

Real separate processes hold and contend for the managed-world lease. Rename
and native regional backup reread authoritative profile state under ownership
and retain the guard through publication. Active sessions and concurrent
maintenance refuse without modifying the original. Busy backup allocates no
output. Tests cover clean/abrupt release, eventual successful maintenance and
nested publication failures, including preservation of useful completed inner
artifacts.

Maintenance can create the stable private lease marker for a never-started
world, but creates no launch witness or progress and claims no checkpoint.
JSON profiles gain rename/lease safety; native JSON backup is not added. Explicit
backend online SQLite backup remains distinct from the native profile-plus-save
wrapper. Nonparticipating older writers, arbitrary administrative replacement
and network filesystems are outside the cooperative ownership claim.

### Desktop startup bounds

Real subprocess fixtures cover normal and failed help, oversized output,
timeout, inherited output pipes and redirected background children. The new
reader retains at most **65,537 bytes**, including the first overflow byte,
without an unlimited temporary spool. The finite deadline covers both process
exit and EOF. POSIX success requires the owned process group to be absent;
cleanup requests are distinguished from observed exit. Windows descendant
certainty remains explicitly narrower.

Passive desktop verification still executes no binary. Explicit help smoke is
a command-line startup check, not a native game-window, physical-device or
publisher-authentication claim.

## Fresh native review and final confirmation

The [native record](verification/v0.23-native/README.md) separates the exact 0.22
baseline, 0.23 candidate, one copy-correction batch and final direct processes.
Actual Linux X11 windows used Vulkan llvmpipe / LLVM 20.1.2, Mesa 25.2.8 software
rendering at **1440×900** and **1100×700 with 150% text**. Every capture has a
fresh PNG, hash, size, dimensions and window PID checked before and after.

The four baseline captures came from the exact delivered 0.22 executable. An
initial setup attempt lacked a Vulkan driver and failed; it is retained, with
its documented helper-label error, and excluded from successful native evidence.
The successful fresh baseline saved and exited zero. Current-checkout changes
during baseline work never change that binary's historical status.

### Candidate journey

The candidate created a managed regional world through character/settings/review,
saved 150% controls, accepted the four-wood Riverside request, traveled, delivered
and received **6.13 credits plus 40 XP**. It then founded **Review23**, granted
10 further XP, moved 50 credits into its treasury and contributed a further 25.
Nature identified **Alex** as the consuming person, explained reach recovery and
showed carried supplies/thresholds beside disabled consumption actions.

The world was saved, renamed, backed up at **checkpoint 23**, verified and
recovered into a separate world. All **52 candidate images** were independently
inspected. The one correction batch clarified effective sky state under
Performance, singular completed-contract grammar and readable item names.

The [candidate integrity audit](verification/v0.23-native/candidate-evidence-audit.md)
checks all 52 images. Launcher PID 11234 has a direct process record and exit 0.
Twenty-three gameplay images observe PID 12147, but no retained child process
receipt independently associates it with the parent, executable or exit status.
The candidate's unchanged-input statement lacks a retained source-map digest.
Those limitations are preserved rather than reconstructed or used to authorize
final delivery.

### Final direct-process confirmation

The rebuilt final game, **PID 14073**, directly reopened the recovered managed
world, confirmed its progression and treasury, inspected the corrected labels at
both supported review sizes and saved/exited **0**. A separate final launcher,
**PID 14392**, confirmed the polished settings and exited **0**. The ten final
images were independently inspected. Both processes retained the same **381
implementation inputs** throughout.

Final native executable SHA-256:
`432748a47f507d23e3efdbd2862f4e5beaae68c41801eeaf4a057392d4f3c99f`.

Recorded canonical source-map identity for both final processes:
`dd6d40911a1defed04d39a3a4287ae01ced1cfde27fdcbf0575caa404c39c5d5`.

Records: [direct game](verification/v0.23-native/confirmation-recovered-process.json)
and [direct launcher](verification/v0.23-native/confirmation-menu-process.json).
The [accepted provenance audit](verification/v0.23-native/confirmation-provenance-accepted.json)
binds these identities to the actual successful terminal receipt, identical
progress alias, all 44 gates, current frozen sources and the accepted executable.
The final-confirmed and accepted source/executable identities match exactly;
no post-confirmation implementation correction or receipt reconstruction was
needed. The earlier pending and negative-admission records remain retained.

### Read-only persistence evidence

The [state inspection](verification/v0.23-native/state-inspection.md) verifies
SQLite integrity and complete logical original/backup equality at checkpoint 23.
It deliberately refused immutable inspection while the recovered database had
an active WAL, then ran a fresh inspection after clean exit. The recovered world
advanced from 421.499598 to 532.949492 simulation seconds and reached checkpoint
35. Full state equality after resumed simulation is not asserted.

| Persisted invariant | Original | Backup | Recovered after final confirmation |
|---|---:|---:|---:|
| Completed supply contracts | 1 | 1 | 1 |
| Community XP | 50 | 50 | 50 |
| Personal credits | 181.13 | 181.13 | 181.13 |
| Review23 treasury credits | 75.00 | 75.00 | 75.00 |
| Founded settlements | 1 | 1 | 1 |

All eight inspected database/journal/backup-record inputs retained identical
bytes, sizes and modes before/after the final read-only inspection. No private
save payload is copied into the source handoff.

## Evidence scope and remaining qualification

The native journey shows out-of-range delivery becoming travel guidance, then a
delivery action after arrival; it does not stage every capacity or concurrency
failure visually. Receiver/wallet capacity, expiry, disappearance, same-frame
intent changes and busy maintenance are established by actual domain/native
system/separate-process tests. Final sky images cover Performance with preference
On in pause and launcher settings, not an exhaustive high-preset/toggle matrix.

These are developer-operated finite scenarios on software Linux rendering.
Actual Windows clean-machine execution, physical GPU/controller/wheel/assistive
qualification, uncoached sustained careers, physical frame/storage budgets,
populated-community policy, licensed production content, migration, signing,
public accounts/moderation, WAN capacity and offsite recovery retain concrete
criteria in the [G01–G18 register](REVIEW_GAPS_v0.23.md).

## Review, preservation and delivery

The three specialists and integrator covered product fit, first/returning
journeys, interaction/accessibility, economy/progression, communities/simulation,
vehicles/geography, framework APIs, multiplayer/privacy, concurrency/recovery,
rendering/performance, operations/content and delivery. Scope, researched
patterns, independent corrections and ownership are retained in the
[review](PRODUCT_REVIEW.md), [research](RESEARCH_v0.23.md),
[implementation ledger](IMPLEMENTATION_PLAN_v0.23.md) and specialist reports.

The exact **2,790-file** 0.22 baseline is checked against an explicit reasoned
preservation policy. The prior current review/validation are retained byte-exact.
The stale root manifest was removed only after byte-and-mode equality with its
retained copy was verified. Original assets, datasets, authoring sources,
licenses and historical evidence stay intact. Final archive inventory binds the
new reports and native evidence separately from implementation acceptance.

The source packer was independently reviewed twice. Before acceptance existed,
its actual invocation refused the missing terminal receipt, exited 1 and created
neither a ZIP nor a success report. This negative test is not successful artifact
acceptance. The [delivery record](verification/v0.23-delivery/README.md) carries
the separate archive and fresh-extraction results, passive verification and
explicit bounded help smoke. The source archive's external verification JSON
binds its final byte identity without a circular self-hash claim. Desktop
consistency, native rendered play and canonical implementation acceptance remain
separate evidence with matching executable identity.
