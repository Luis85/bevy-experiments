# Validation and release evidence — 0.14

Review date: **2026-10-28**. **All 26 frozen-source gates and the accepted-executable
native confirmation passed:** 828 distinct Rust tests, 895 passing executions,
zero failures or ignored tests, and 347 unchanged inputs. Actual results are in
[the gates](verification/v0.14-final/gates.json),
[native confirmation](native-v14-verification.json) and the
[release record](release-verification-v0.14.json). The
[0.13 validation](VALIDATION_v0.13.md) and
[product review](PRODUCT_REVIEW_v0.13.md) retain their original evidence.

## Source, environment and acceptance identity

| Contract | Recorded value |
|---|---|
| Release | 0.14.0 |
| Wire protocol / TLS ALPN | 10 / `terra-world/10` |
| Domain save / regional schema | 1 / 1 |
| Actual compiler | `rustc 1.99.1 (58cf8f9c3 2026-10-13)` |
| Frozen source/support inputs | **347** |
| Final source-unchanged check | **Passed: all 347 inputs unchanged through gates** |
| Built native SHA-256 | `836521f99cbfb714fc23214d760f43af4bef1495d2c5007a1d53e879c5e8f531` |
| Final distinct Rust tests / passing executions | **828 / 895** |
| Final failures / ignored tests | **0 / 0** |
| Automated environment | Linux x86-64, cached dependencies, three build jobs |
| Native confirmation environment | Xvfb; Vulkan llvmpipe, LLVM 20.1.2; Performance quality for final confirmation |
| Build profile | Optimized dev profile, `opt-level = 1` |

The [source manifest](verification/v0.14-final/source-hashes.json) covers
workspace manifests, Rust source, embedded fixtures, model/terrain inputs,
configuration and support/verification scripts. Changing prose and generated
reports are excluded; the release record hashes the final documents and native
and package evidence separately.
The verifier checks the actual compiler against the pinned toolchain, captures
command output and artifact hashes, and deduplicates passing tests by crate,
harness and case. Repeated report gates are executions, not extra distinct tests.
All published gate reports and logs also retained their captured hashes through
completion at 20:37:18 UTC. All four final native runs used the exact executable
hash above and retained unchanged source inputs.

## The 26 release gates

Run command:

```sh
python3 scripts/verify.py --offline --jobs 3 --report-dir docs/verification/v0.14-final
```

A new run requires a new evidence directory. Offline execution needs cached
Cargo dependencies; TCP/TLS cases require localhost socket access. The existing
[gates record](verification/v0.14-final/gates.json) is the authority for exact
commands, exit status, timings and log hashes.

<!-- final-gates:complete -->
| Gate (all passed) | Passing test executions | Elapsed seconds |
|---|---:|---:|
| compatibility | 0 | 0.062 |
| format | 0 | 1.615 |
| headless | 570 | 452.276 |
| career-report | 1 | 54.528 |
| purchased-career-report | 1 | 3.034 |
| novice-career-report | 1 | 46.267 |
| traffic-recovery | 3 | 19.316 |
| bevy-headless | 92 | 241.518 |
| bevy-render | 95 | 313.852 |
| demo | 131 | 40.274 |
| demo-check | 0 | 85.378 |
| headless-example | 0 | 0.486 |
| custom-provider-example | 0 | 0.453 |
| asset-pack | 0 | 0.245 |
| asset-inspect | 0 | 0.237 |
| asset-build | 0 | 0.256 |
| asset-authored-validate | 0 | 0.210 |
| source-inspect | 0 | 0.169 |
| source-export | 0 | 0.150 |
| source-install | 0 | 0.180 |
| source-verify | 0 | 0.127 |
| recovery-probe | 0 | 11.581 |
| recovery-bundle | 0 | 1.920 |
| session-probe-checks | 1 | 0.572 |
| regional-sessions | 0 | 129.196 |
| native-build | 0 | 44.320 |
<!-- final-gates:complete:end -->

The [identity list](verification/v0.14-final/distinct-tests.json) contains all
828 distinct passing cases:

| Crate | Distinct passing tests |
|---|---:|
| `terra-bevy` | 126 |
| `terra-cli` | 4 |
| `terra-core` | 33 |
| `terra-demo` | 131 |
| `terra-history` | 13 |
| `terra-net` | 121 |
| `terra-persistence` | 35 |
| `terra-sandbox` | 320 |
| `terra-stream` | 45 |

Cached compilation did not replace execution. Earlier candidate socket failures
were resolved by an allowed localhost rerun; the frozen-source gates above
executed independently.

## Company guidance and native integration

All **nine new Company domain cases** passed in the frozen-source headless gate.
They cover ordinary paid setup, owner-private service selection, accepted-work
and urgent-need precedence, loaded hold/save/resume, stale assignment detection,
remote rental renewal, unavailable source advice, finite money/capacity limits
and legacy accounting absence. Read-only guidance remains outside saved
authority; it cannot create money or cargo or resume a service. See
[the cases](../crates/terra-sandbox/tests/company_guidance.rs) and
[their captured execution](verification/v0.14-final/headless.log).

The native regressions passed in the 131-test
[demo gate](verification/v0.14-final/demo.log), covering exact Fleet/Recovery
and Garage selections, clearing unrelated confirmations and principal/restore
context. The prepared journey starts near the garage; its ordinary approach
requires one **0.05-second** step. This is an interaction setup check, not a
substantial walking benchmark. It uses the ordinary 250-credit wallet, pays
registration and recruitment, transfers operating cash, assigns a real route
and drives to actual passenger pickup before holding the load. Its ready state
retains 130 personal credits and 90 company credits. Generated states remain
private review inputs; no balance, position or status is rewritten to force
progress. The final demo gate passed this journey.

The [candidate review](verification/v0.14-native/v14-review-company-process.json)
opened the held service's exact Recovery section at 1440 × 900 and exercised
focus, scrolling and cancellation at 1100 × 700 with 150% text. Its
[nine subsystem comparisons](verification/v0.14-native/v14-company-review-conservation.json)
were exact; other environmental records could grow through streaming. That run
preceded the final wording and output-path polish and remains candidate evidence.

The accepted executable repeated the native journey on the **recovered prepared
world** from an unrelated working directory. The
[focused Company action](screenshots/v14-confirm-company-focus.png),
[scrolled recovery action](screenshots/v14-confirm-recovery-action.png) and
[named confirmation](screenshots/v14-confirm-return-review.png) remained usable
at 1100 × 700 with 150% text. Selecting Company canceled the review before a
command was committed. F5/F9 and keyboard save-and-quit were exercised. The
[final comparison](verification/v0.14-native/v14-confirm-company-conservation.json)
retained all nine subsystems **and the complete export byte for byte**, including
130 personal credits, 89.95 company credits, one held passenger and its 8.64-credit
reservation. The process exited 0.

Fresh menu, character customization and world creation were also reviewed.
Those ordinary menu inputs produced the procedural recovery case below. The
[post-interruption view](screenshots/v14-confirm-retained-restart.png) confirms
that the final held-service explanation points to **Resume delivery / return in
Recovery**. Complete device/accessibility and uncoached usability qualification
remain open.

## Fresh commercial and movement measurements

All three career reports were rerun by the current gates. Their parsed results
match the corresponding 0.13 reports exactly; these are new executions with
retained historical comparison, not copied acceptance.

The [passenger career](verification/v0.14-final/novice-career-report.json)
completed **65 deliveries over 3,600 service seconds**, finished its commitment
and physically returned the rental by **3,649.5 seconds**. From an ordinary
250-credit wallet, it earned 821.60 in public fares and paid 127.12 in known
registration, recruitment, payroll, lease, net fuel and return-damage costs:
**694.48 credits net**. No later owner deposit or starter-vehicle consumption
was needed. Six ten-minute JSON resumes and a regional accounting roundtrip
were exact. It conserved 13,650 credits including escrow and 160 passengers.

Five explicit ten-second hold/resume interventions were necessary. Ecology time
was zero; living costs and management time remain unpriced, and public funding
is finite. The result establishes one managed strategy, not unlimited economic
sustainability. The [contributed-stock](verification/v0.14-final/career-report.json)
and [purchased-stock](verification/v0.14-final/purchased-career-report.json)
warehouse runs still return **−84.69** and **−161.49 credits** after recorded
owner costs. Each completed 24 deliveries across six sessions and retained its
conservation, scarcity, recovery and resume checks.

The current [traffic replay](verification/v0.14-final/traffic-recovery.json)
passed three tests. The two retained loaded fixtures physically returned in
34.0 seconds (`rear-trap`) and 49.5 seconds (`occupied-exit`), preserving the
12-wood commitment, 770 total goods, 1,500 credits and exact save/resume behavior.
The additional generation-251 walking replay belongs to 0.13 and **was not
replayed for this release**; its prior measurements must not be described as
current acceptance. General movement regressions remain in the ordinary suites.

## Asset authoring and prepared-source contracts

All four asset gates passed. [Original-pack validation](verification/v0.14-final/asset-pack.json)
accepted five unchanged models. [GLB inspection](verification/v0.14-final/asset-inspect.json)
verified the original deer bytes and all five clips (`idle`, `walk`, `run`,
`feed`, `drink`), reporting 37 nodes, 1,800 scene triangles and 1,190 animation
keys. [Recipe build](verification/v0.14-final/asset-build.json) admitted all five
sources, and [runtime revalidation](verification/v0.14-final/asset-authored-validate.json)
accepted the exact generated pack with matching identity
`sha256:58e4c7203eaea18b5cc337243d70a0918211a2770fc7c172fa5b0b9a71143634`.

All nine authoring integration cases passed, covering deterministic output,
snapshotted bytes, path/size limits, unused-animation/hierarchy budgets, distinct
binding policies for shared bytes, CLI behavior and the original example.
The accepted native executable loaded this generated pack explicitly. Its
[final telemetry](verification/v0.14-native/v14-confirm-prepared-company-telemetry.json)
reported **22 ready instances**, zero loading/fallback/rejected instances, zero
failed models and no asset error. The interrupted-world restart reached the same
ready state. The original pack and model bytes remain unchanged. Importer and
native integration acceptance do not qualify production human/boarding/audio
content, declared licenses or target-GPU capacity.

The four source gates also passed. [Inspection](verification/v0.14-final/source-inspection.json),
[export](verification/v0.14-final/source-export.json),
[installation](verification/v0.14-final/source-install.json) and
[verification](verification/v0.14-final/source-verify.json) retained identical
source/package identities and complete **81-of-81-tile** coverage at level 15,
with no missing tiles. The input comprises two adjacent synthetic bundles,
seven files and 115,316 bytes. This verifies the declared authored fixture,
not surveyed accuracy, NoData handling or a licensed shipping region.

## Recovery correctness and actual saved worlds

The [complete recovery probe](verification/v0.14-final/recovery-bundle.json)
passed on the frozen source, restoring generation 2 across **1,029 nonempty
regions**, with
4,096 resources (0 active), 52 living bodies, two principals, one faction and one
shared plot. The retained set contained 12 files and 5,327,925 bytes. All 15
verification flags passed after the original database/configuration/source tree
was removed, including full cold state, local metadata, catalog/terrain identity,
exact political input and authored politics, unchanged retained bundle and
rejection of a damaged copy without output. Unix private permissions were checked.

The final run took **649.445258 ms to create** and **368.48962 ms to restore**, including
validation on this machine. Those are local maintenance measurements, not RPO,
RTO, offsite service or power-cut qualification. Distant resource records do not
claim terrain coverage at their locations.

Two separate real-input rehearsals have completed export comparison:

| Rehearsal | Recorded result | Accepted native result |
|---|---|---|
| Ordinary menu-created procedural world, character Alex | Removed the actual menu world plus resolved config/political copy. Generation 40 restored; complete before/after exports have the same SHA-256. Character customization and default political input are retained. | Reopened all 49 chunks from the restored config; normal saved activity visible; clean exit. |
| Native-reviewed prepared Company world | Removed an exact private input copy and its source tree. Generation 31 restored; complete before/after exports have the same SHA-256 and original prepared identity. Political input remains absent. | Reopened with `--no-default-politics`, exact Company handoff, complete export retained and clean exit. |

The [ordinary-menu summary](verification/v0.14-native/v14-menu-recovery-summary.json)
and [prepared-company summary](verification/v0.14-native/v14-prepared-recovery-summary.json)
retain exact receipt/export hashes. Both final native runs used absolute restored
configurations from an unrelated working directory. Private complete exports,
profiles and databases remain outside the release. The ordinary menu case checks
the newly included default political dependency; the prepared case checks that
startup does not add an unintended dataset. Runtime simulation can advance after
an unpaused world is reopened; exact recovery comparison occurs before play.

The separate [storage-only probe](verification/v0.14-final/recovery.json) passed
all six verification flags at generation 2: 1,027 nonempty regions, 6,144
resources and 47 living bodies. It preserved complete state/private metadata,
cold records and source bytes, and rejected corruption without output. Backup,
recovery-copy and restore measured 47.019, 55.125 and 34.132 ms. Unlike the local
bundle validator, this generic receipt still requires host-specific metadata
validation.

The [TLS session workload](verification/v0.14-final/regional-sessions.json)
passed with 16 real localhost clients, eight churn cycles, 4,840 advancing steps,
prepared-source loss/restoration and 1,024 cold overlays. It retained 64 exact
offline-owner checks, 64 resume checks, 128 restart checks and 16 authenticated
owners after backup recovery. The injected storage failure was atomic and its
retry exact; cargo, crafting sequences, private metadata and dormant records
were conserved. The backup was generation 165; final storage reached 169.

| Measured TLS workload result | Value |
|---|---:|
| Advancing host-update p50 / p95 / p99 | 16.082 / 44.349 / 52.633 ms |
| Durable acknowledgement p95 / p99 (128 samples) | 99.489 / 101.143 ms |
| Ping roundtrip p95 / p99 (6,464 samples) | 56.748 / 67.519 ms |
| Storage-paused updates | 2,136 |
| Process RSS before admission / after recovered resume | 36,872 / 102,340 KiB |

These finite shared-container results include host and client fixtures. Storage
still pauses authority; RSS samples do not establish a long-duration plateau.
No Internet delay/loss, GPU rendering or public-service capacity was tested.

## Diagnostics and output protection

The final prepared Company session retained 16 samples and finished at sequence
42 with `graceful_exit`, no skipped captures and no prior write failures. Its
clock remained paused while periodic wall-clock capture continued. Tests cover
retention across runs, failed publication preserving the prior report, failed
final checkpoint/export, and save/database aliases. The final demo gate also
passed three cross-review regressions for screenshot parents, derived political
export parents and current/future session namespaces.

The [owned-process interruption](verification/v0.14-native/v14-interruption-event.json)
terminated the accepted executable with SIGKILL before final export. Its private
diagnostic file remained readable with **15 complete running captures**. The
[last complete report](verification/v0.14-native/v14-interrupted-last-complete.json)
was retained as evidence; it does not claim a graceful or healthy session.

The [same-file restart](verification/v0.14-native/v14-interruption-continuation.json)
reopened the world successfully and retained **14 prior captures plus two new
captures**, within the 16-entry capacity. The latest capture records a clean
checkpoint and `graceful_exit`; the run reported no world, simulation, storage
or asset failure. Both sessions used the accepted binary and unchanged source.
This is a process-termination rehearsal, not a machine power cut, crash dump,
continuous trace or monitoring service. A stale or absent capture remains unknown
health. Diagnostic paths/source identifiers still need review before sharing.

## Preservation, delivery and qualification

The [baseline comparison](verification/v0.14-final/baseline-comparison.json)
binds the exact accepted 0.13 ZIP and preserves all **834 prior paths**, all
**90 original asset/data/test-fixture inputs**, and all **447 historical
report/evidence files**. The prior current review and validation were also
archived byte-exactly. The
[source archive manifest](source-archive-manifest-v0.14.json) inventories the
complete final project with per-file hashes; it excludes only itself.

The Linux desktop package contains the exact accepted optimized-dev executable,
curated runtime data, licenses, player guides and complete gate evidence. The
separate [extracted-package record](desktop-package-v0.14.json) records archive
integrity, launchers, native startup, original models and execution from outside
the source checkout. The source project supplies the SDK and operator/authoring
programs; their executables are not included in the desktop package. Private
review worlds, user profiles, credentials and build/dependency caches are excluded
from both deliverables. Artifact hashes are bound in the release record and the
source ZIP's manifest.

The [product review](PRODUCT_REVIEW.md) and [G01–G18 register](REVIEW_GAPS_v0.14.md)
retain Windows, physical hardware, accessibility, production content, offsite
recovery objectives and public-service limits. Linux software rendering and
finite local tests do not establish those separate qualifications.
