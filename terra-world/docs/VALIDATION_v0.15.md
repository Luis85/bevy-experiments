# Validation and release evidence — 0.15

**Automated and native acceptance passed.** The final 0.15 run completed all
34 gates on 356 unchanged implementation inputs. It recorded **860 distinct
Rust tests, 929 passing executions, zero failed and zero ignored tests**.
Three native confirmation processes used its exact executable and exited 0.
The [release record](release-verification-v0.15.json) binds these results to
[native evidence](native-v15-verification.json), the separate
[extracted Linux package checks](desktop-package-v0.15.json) and the final
[preservation comparison](verification/v0.15-final/baseline-comparison.json).
The prior accepted [0.14 validation](VALIDATION_v0.14.md) remains unchanged.
Review date: **2026-10-29** (Europe/Berlin); process timestamps remain UTC.

The scope is a regional SDK and playable reference game: bounded stopped-body
navigation, an original motion-driven Comet pack, native managed-world recovery
and the resulting review/polish corrections. The [gap register](REVIEW_GAPS_v0.15.md)
retains G01–G18 and their broader closure criteria. See the
[product review](PRODUCT_REVIEW.md) for the observed user experience.

## Reproducible automated gate

```sh
python3 scripts/verify.py --offline --jobs 3 --report-dir docs/verification/v0.15-final
```

The runner uses the pinned **Rust 1.99.1** compiler and the locked dependencies.
It captures every command, exit status, test identity, log digest, report digest
and implementation-input hash. Its 34 declared gates cover compatibility,
formatting, headless/domain tests, three priced career measurements, existing
traffic recovery, the new cluster journey, headless and rendered Bevy tests,
native integration, managed recovery, examples, original and new asset packs,
reproducible vehicle generation, source distribution, complete local recovery,
real local TLS session cycling and the final native executable build.

The final [gate record](verification/v0.15-final/gates.json) is the authority for
completion and counts. A command without its completed successful result is not
an accepted gate. The runner distinguishes distinct Rust tests from repeated
passing executions across feature configurations and focused report gates.
Native and desktop acceptance bind the exact resulting executable separately.

The final compiler is `rustc 1.99.1 (58cf8f9c3 2026-10-13)`. The accepted
executable SHA-256 is
`c12970d499bd2f9045cad0f9c106cdc870dddd64ef7804fd496d8b37c96cc504`.
All gate logs, published reports and source hashes were rechecked. Tests by
crate are recorded without double-counting repeated configurations:

| Crate | Distinct passing tests |
|---|---:|
| terra-bevy | 137 |
| terra-cli | 4 |
| terra-core | 33 |
| terra-demo | 146 |
| terra-history | 13 |
| terra-net | 121 |
| terra-persistence | 35 |
| terra-sandbox | 326 |
| terra-stream | 45 |
| **Total** | **860** |

## Current measurement environment

The recorded host uses Linux 6.18.55, Python 3.12.14 and the pinned Rust compiler.
Native interaction uses X11/Xvfb with Mesa llvmpipe software Vulkan, Performance
quality and the original local prepared/procedural sources. Reviewed window
sizes are 1440 × 900 and 1100 × 700; enlarged text reaches 150%. There is no physical
GPU, controller or steering-wheel qualification in this environment.

Native screenshots are real window captures. Input is scripted through the
observed interface, with prepared fixtures for later-state content and recovery
checks. They are not an uncoached player study. Frame settling and rapid input
coalescing on the shared software-rendering host must not be reported as target
hardware performance. No new benchmark throughput promise follows from these
screenshots or from import-budget estimates.

## New physical navigation case

The [cluster report](verification/v0.15-final/navigation-cluster.json) measures
an unchanged 60-metre goal obstructed by three stopped bodies. The fixture uses
0.1-second physical steps on bounded authored flat support. It arrives in 145
steps, or 14.5 physical seconds, after travelling 75.48866 metres, ending
0.26763 metres from the goal. Maximum lateral excursion is 18.65695 metres,
maximum step distance is 0.800000002 metres and maximum heading change is
3.36781 degrees. At most three saved path points are present in that case;
the general path limit remains 129.

The case records 10,512 source queries, at most 5,760 in one update against its
16,384 ceiling. It verifies supported complete envelopes, clear body sweeps,
unchanged parked bodies, conserved goods and money, bounded source work, exact
143-step save/resume continuation and a valid complete state. Its ten verification
flags are true. The gate checks the exact selected test and one anchored report.

The retained [baseline](verification/v0.15-review/navigation-015-baseline.json)
uses original 0.14 source and unbounded flat support. It advances only 7.30 metres
in 120 physical seconds. The final fixture additionally bounds its supported
region. This is a geometry regression and physical-time comparison, not a
hardware timing benchmark, prepared-region coverage guarantee, general road
planner or company profitability result. Full reconnect admission and per-update
planning limits received independent cross-review.

## Repeated economy and traffic measurements

All three career measurements executed again. Their parsed reports exactly
match the retained 0.14 results; the existing traffic cases match when indexed by
fixture name (parallel output order differs). The
[comparison receipt](verification/v0.15-review/repeated-measurements.json)
retains both report digests and the explicit comparison method.

The current [novice passenger](verification/v0.15-final/novice-career-report.json),
[contributed-stock](verification/v0.15-final/career-report.json) and
[purchased-stock](verification/v0.15-final/purchased-career-report.json) reports
remain finite managed strategies with their original accounting limits. The
passenger case completes 65 deliveries over 3,600 service seconds and returns its
rental by 3,649.5 seconds, with 694.48 credits net after the recorded known costs.
Five explicit ten-second hold/resume interventions remain necessary. Ecology time
is zero; living and management costs remain unpriced and public funding finite.
Those constraints have not been erased by the new detour case.

The two original loaded-queue recovery cases still take 34.0 seconds for the rear
trap and 49.5 seconds for the occupied exit. They retain physical return, accepted
obligations, finite goods and money. Their existing fixture geometry remains
unchanged; see [traffic recovery](verification/v0.15-final/traffic-recovery.json).

## Vehicle content and authoring acceptance

The new checks reproduce the original Comet generator's four outputs,
[inspect its GLB](verification/v0.15-final/vehicle-inspect.json), validate the
model-specific standalone pack and the six-model direct
manifest, build the composed recipe and revalidate the exact published pack.
The GLB has 91 nodes, 78 meshes, 2,276 triangles and named `parked`/`rolling` clips;
its 210,148 bytes hash to
`e13f7026ad6bee911f52c664e480cfd5f42a3417d2ecd463ea755b7ce72479c4`.

Direct and recipe-built packs have distinct normalized declarations and paths,
so their identities legitimately differ. The accepted default wildlife identity
remains `sha256:406c12494416b472ca5bee80a809e0ef30a45e382240cab01faa9081cd838d6d`.
The original wildlife files and all protected source inputs receive independent
byte-preservation checks. New optional pack features require the 0.15 reader;
old kind/species declarations keep their identities. Gameplay save 1, regional
schema 1, wire 10 and ALPN `terra-world/10` remain unchanged.

Focused and full-suite tests cover model specificity, public appearance lookup,
rig hierarchy/scale/channel rejection, per-instance signed wheel travel,
low-speed/reverse/stop/pause behavior and aggregate work admission. The native
Garage guard checks the actual selected appearance, displayed principal/restore/
vehicle/garage context and existing paint state before producing a paid command.
Procedural paint still costs the ordinary five credits.

The prepared native Comet fixture starts with 25,000 money units and uses an
ordinary 6,180-unit rental payment, including 4,500 units of bond escrow,
leaving 18,820 units. These correspond to 250.00, 61.80, 45.00 and 188.20 credits
respectively. It uses real approach orders and ordinary boarding at
5.51718 metres within the existing 20-metre rule. Each prepared approach takes
0.05 seconds. It is a repeatable later-state presentation fixture, not a
substantial walk or career benchmark. The
[source fixture](../crates/terra-demo/src/frontier.rs) and its exact receipt
remain in the project.

## Managed recovery acceptance

The exact [managed-world report gate](verification/v0.15-final/launcher-recovery.json)
backs up generation 2 with two principals,
52 living bodies and 16 resource patches across 19 nonempty regions. It records
57 paged records and 25 globals. The original world and its original prepared and
political inputs are removed before restore. The report checks the full archive,
source identity, political input, character/presentation, rebased absolute paths,
checkpoint status, published profile discoverability, unchanged retained backup
and a subsequent separate backup of the recovered world.

Selection, changed metadata/checkpoints, unsupported/damaged/linked/incomplete
inputs, protected database namespaces, politics-free profiles, bounded worker
admission and navigation during completion have focused regressions. The
completed-result test supplies 24 future-dated entries and verifies that the exact
new backup or recovered profile remains reachable within the 24-item limit.
The larger backend original-tree-loss rehearsal remains a separate gate; native
wrapping does not expand the backend's supported host/source/catalog scope.

A recovered profile is a new managed item and receives its own creation time.
The original creation timestamp remains in the backup record. Character,
quality and initial-hour preservation must not be confused with timestamp
identity of the new profile. Native recovery does not migrate world content.

## Candidate native review and the polishing correction

Three candidate processes exited 0. Their executable SHA-256 is
`cf9f03f18ca77d714bbdbaf482717f2281d8e0d72c4de4cd43546fa8fba3e80e`.
They remain exploratory evidence; the Comet run's implementation-input snapshot
changed while the independently reviewed HUD correction was made. No running
binary was replaced, and those observations are not relabeled final acceptance.

The ordinary menu journey changed the character outfit, created a Performance
world, accepted the four-wood supply request, saved, created a checkpoint 34
backup, displayed an actionable empty-folder error and verified a named review.
Keyboard cancellation left one managed world. A fresh review restored a new
copy without opening it automatically. Native reopening displayed the retained
four ready wood and the 176-metre destination. The original remained selectable.

The Comet review loaded its paid snapshot through ordinary F9. The optional pack
showed six models and 23 ready third-person instances with zero failed/rejected
models. Reverse slope rolling, forward throttle, parking to 0 km/h and the existing
first-person cockpit were observed. First-person hides the possessed exterior,
leaving 22 ready instances. Garage visibly explained the fixed livery and disabled
paint. Final diagnostics and exit status were clean.

The visual pass found the World drawer overlapping the left driving card and
leaving chopped text visible. The correction makes both floating driving cards
follow the same existing rule when World, Inspector or Performance is open.
The footer and panel controls remain available. Nine focused driving tests,
including actual ECS visibility combinations, and the existing enlarged-footer
layout test pass. The final accepted executable confirmed the correction for World, Inspector and
Performance at 1100 × 700 with 150% text; both cards returned after drawers closed.

A separate candidate reviewed the original-loss backup at 1100 × 700 and 150% text.
The list, named review, complete scrolled location and persistent Back/Recover
controls were readable. Keyboard cancellation and quit retained one recovered
profile and two backups. Screenshots, process logs and the
[independent product review](PRODUCT_REVIEW.md) preserve the exact observed scope.

## Final native confirmation and local session workload

The [native record](native-v15-verification.json) binds three clean processes to
the gate executable: enlarged-text launcher recovery, a reopened recovered
prepared world, and the paid Comet presentation/HUD review. All ran from an
unrelated working directory with the same 356 inputs. The
[pre-open recovery receipt](verification/v0.15-native/v15-confirm-recovery-exact.json)
checks every schema definition and every row/column in the restored database,
including 19 region rows and one world row, against the immutable selected
backup. All 42 pre-existing files were unchanged both before and after opening.
Character/quality/hour, all copied source/political data and rebased paths matched.
The new profile correctly has a new creation timestamp.

The recovered world reached 49 rendered chunks with regional restoration true.
Comet reached 23 ready third-person instances; first person had 22 ready and
one deliberately hidden exterior counted in fallback accounting. Failed models,
rejected instances and world/simulation/asset errors were zero or null. Both
world processes retained `graceful_exit` diagnostics. The final Comet review was
paused; forward/reverse/parking observations belong to the earlier candidate,
while signed wheel and pause behavior also has final automated coverage.

The final [local TLS workload](verification/v0.15-final/regional-sessions.json)
used 16 real clients, eight churn cycles, 4,840 fixed steps and 1,024 cold overlay
regions with 4,096 cold resources. The complete world held 5,248 resource nodes,
182 living bodies and 80 owned bodies, with 25 resident prepared-source chunks.
All exact offline/resume/restart, durable-acknowledgement, cargo, source-loss and
retry checks passed. The final store retained all 1,024 dormant regions and
4,096 dormant records.

This run took 216.739 seconds wall time. Its 128 durable-command samples had
110.799 ms p95 and 115.095 ms p99 acknowledgement; 6,464 ping samples had
147.937 ms p95 and 185.266 ms p99 round trip. The shared host also performed
native review during part of the run. These are recorded local observations,
not isolated hardware benchmarks, a regression attribution, WAN qualification
or a supported-client promise. Earlier measurements remain archived unchanged.

## Earlier preparation and environment issues

The first full 0.15 release run was interrupted after 31 completed gates
when its execution environment no longer retained the runner process. Its
implementation inputs still matched the frozen snapshot. The partial record is
preserved under `verification/v0.15-interrupted/`, with an explicit
[interruption receipt](verification/v0.15-interrupted/interruption.json).
That record is not accepted release evidence. The complete fresh run subsequently
passed all 34 gates in the final directory.

The first shared native warm build ran while launcher work was being edited and
did not complete; it was a dependency warm-up, not a release gate. A focused
renderer run passed 99 non-network tests while two existing UDP bind tests were
blocked by sandbox permissions. The final full gates ran with permission for
local networking and passed both tests with no ignored or failed result.

The initial Comet preparation test incorrectly required a three-metre boarding
approach. It was corrected to the existing 20-metre command contract, and the
paid ordinary boarding test passed at 5.51718 metres. No balance or position was
forced to obtain the fixture. One exploratory SQL/profile comparison similarly
overconstrained the new recovered profile's creation timestamp: every committed
SQLite row compared equal before reopening, while character/quality/hour were
unchanged and the new profile correctly had a new timestamp. Final confirmation
uses the actual contract and retains its comparison receipt separately.

## Delivery and preservation

Delivery comprises the exact accepted binary in a Linux x86-64 review package,
the complete source ZIP and a standalone improvement/open-gaps report. The
[desktop receipt](desktop-package-v0.15.json) binds the extracted-folder checks;
the [source comparison](verification/v0.15-final/baseline-comparison.json) checks
all 959 prior archive paths, 90 protected original inputs and 560 historical
evidence files, with exact archives of both prior current review documents. The archive manifest hashes every payload member
except itself; an external record binds the finished ZIP digest and entry checks.

The desktop packager checks frozen source/log/report/binary hashes and a curated
payload. The separate extracted-folder confirmation checks every archive member
and runtime dependency, starts from an unrelated directory, uses packaged source
and presentation content, and records any checkout-path access. Packaging is an
acceptance step after successful gates, not a substitute for native execution.

## Evidence limits

- The Linux artifact uses the project's optimized dev profile. Release-profile
  performance, universal distribution portability, Windows execution, signing,
  installers and updates require their own acceptance.
- Native software Vulkan and scripted input do not qualify physical GPUs,
  assistive technology, controllers/wheels or uncoached player understanding.
- The Comet is original representative content with an empty cabin and fixed
  livery. Human rigs, occupants/boarding, attachments, audio and production LODs
  remain open.
- Local backups are not offsite retention, encryption, an authenticated publisher
  service or deployment RPO/RTO. Historical/live/server/custom-catalog policies
  remain explicitly unsupported by this automatic workflow.
- The bounded cluster case and local TLS load do not establish dense-road
  autonomy, WAN populations, unlimited economic growth or global authority.

The [complete gap register](REVIEW_GAPS_v0.15.md) is the remaining work contract.
