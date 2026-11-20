# Validation — Terra World 0.24.0

Executed on Linux x86_64 on **30 October 2026**, with pinned Rust **1.99.1**, locked offline dependencies and three Cargo build jobs. All receipt timestamps use UTC. The exact preceding release record remains at [Validation 0.23](VALIDATION_v0.23.md).

## Canonical acceptance

**All 44 required gates passed.** The run began at **13:13:54.011453 UTC** and completed at **13:37:40.812703 UTC**, taking **1,426.801 seconds**. The terminal receipt records **1,099 distinct passing Rust tests** across **1,200 passing Rust test executions**, with zero failed or ignored Rust tests. The separate Python tooling gate passed **72 tests**; those are not included in the Rust totals.

The [completed terminal receipt](verification/v0.24-release-acceptance/completed-gates.json) binds **390 unchanged implementation inputs**, **36 evidence artifacts**, complete gate/startup logs and the exact final native executable. Published reports and logs remained unchanged. Every gate and both startup probes recorded complete output, observed leader exit and an absent ordinary owned POSIX process group. No gate timed out or exceeded its output bound.

Terminal receipt SHA-256: `8c386ac3ef5f1bead91f808db90b1301a6b6b834096a36c2cc654abd8f776d81`.

| Crate | Distinct passing Rust tests |
|---|---:|
| terra-sandbox | 393 |
| terra-demo | 250 |
| terra-bevy | 180 |
| terra-net | 128 |
| terra-persistence | 53 |
| terra-stream | 45 |
| terra-core | 33 |
| terra-history | 13 |
| terra-cli | 4 |
| **Total** | **1,099** |

The [distinct-test inventory](verification/v0.24-release-acceptance/distinct-tests.json) deduplicates overlapping configurations and focused gates. Focused preflight results below are not added again.

Command:

```sh
python3 scripts/verify.py --offline --jobs 3 --report-dir docs/verification/v0.24-release-acceptance
```

The unchanged gate roster covers compatibility, Python tooling, formatting, all headless crates, sustained commercial careers, traffic/navigation/guidance, Bevy headless/render adapters, the native demo and launcher recovery, provider examples, original asset inspection/generation/authoring, portable source bundles, recovery probes, real local TCP/TLS sessions and the native executable build.

The [regional session report](verification/v0.24-release-acceptance/regional-sessions.json) completed a **225.070-second finite local TLS workload** with 16 real socket clients, eight churn cycles, 4,840 fixed steps, 80 owned bodies and 1,024 cold overlay regions containing 4,096 dormant records. All real sessions received snapshots; cargo, command sequences, restored owner state, cold records and backup contents remained exact. The expected storage failure remained atomic and retried successfully. This accelerated shared-container workload does not establish concurrent-player capacity, Internet behavior or a long-duration memory plateau.

## Focused implementation acceptance

These are actual focused runs retained separately. Their counts overlap the canonical suite and are not added to its distinct-test or execution totals.

| Focused run | Actual result | Evidence |
|---|---:|---|
| Six supply/guidance suites, including 13 new supply lifecycle regressions | **83 passed**, 0 failed/ignored | [Receipt](verification/v0.24-preflight/world-focused-02.json), [log](verification/v0.24-preflight/world-focused-02.log) |
| Complete native demo suite before the final two-label correction | **250 passed**, 0 failed/ignored | [Receipt](verification/v0.24-preflight/player-native-suite-attempt5.json), [log](verification/v0.24-preflight/player-native-suite-attempt5.log) |
| Persistence crate | **53 passed**, 0 failed/ignored | [Receipt](verification/v0.24-preflight/reliability-persistence-final.json), [log](verification/v0.24-preflight/reliability-persistence-final.log) |
| Python process/receipt/package tooling | **72 passed**, no failures/skips | [Receipt](verification/v0.24-preflight/reliability-python-final.json), [log](verification/v0.24-preflight/reliability-python-final.log) |
| Native final build 03 | Exit **0**, 390 unchanged inputs, exact build-01 source/executable restored | [Build receipt](verification/v0.24-preflight/native-final-build-03.json) |

The domain and native runs retain their actual source maps. The focused reliability receipts bind commands, complete logs and bounded execution metadata, but do not contain source-input maps; final canonical acceptance provides the complete current implementation binding. Earlier failed compile/fixture attempts are retained and explained in specialist reviews. Production protections were not weakened to make fixtures pass.

### Useful supply and escrow continuation

New work uses current useful producer/retail demand and receiving inventory capacity, choosing an amount in 1–12 before reserving reward. Tests cover a one-item shortage, aggregate capacity, full inventories, changed recipes/disabled services, the full 16-slot board, and deterministic maintenance.

Confirmed obsolete public requests retire only after exact cap-safe credit to their original issuer. A missing/capped issuer or unknown receiving view retains value. Existing positive offers keep their terms when demand partially changes or receiving capacity is temporarily full. Accepted commitments remain deliverable on their original terms despite demand changes; expiry/Return releases ownership before public maintenance.

Executed regressions cover exact finite-money conservation, no double refunds, monotonic IDs, spending/retry after a refund cap, competing admissions and validated disk save/restore continuation. The typed `SupplyContractBlocker::ObsoleteOffer` is a public Rust API addition; exhaustive downstream matches must handle it. There are no new serialized fields or save migration.

### Exact native operation intent

Actual Bevy Activate events, queued actions and dispatch exercise unchanged success and changed/missing target rejection. Industry retains the named facility, stable recipe/ownership and intended enabled state. Thirteen routine Fleet operations bind the relevant company, employee, vehicle, route, facility, stock amount/direction and stable assignments. Continuously changing rental countdowns and payroll do not invalidate an otherwise unchanged review; actual expiry and eligibility are rechecked at execution.

Building damage names the exact structure and health/consequence, enters on Cancel and uses a one-use confirmation. Tests cover cancellation, target replacement, health/ownership/reach, body/player/restore changes and same-frame chooser/action races. Missing explicit faction or operational selections remain missing until deliberate browsing. This protection does not imply that every Build action retains a reviewed target.

Local intent tokens are neither remote reservations nor replacements for authoritative command validation and sequencing. Rejected operations preserve the tested inventory, money, movement, ownership and assignments.

### Portable protected outputs

Persistence uses exact already-locked `same-file = 1.0.6` identity for existing files and resolves existing ancestors for new paths. Common tests protect database, sidecar and managed-world identities through alternate names; legitimate distinct outputs remain usable. Windows-specific junction/path fixtures are wired to the existing Windows job, but were not executed on an actual Windows machine here.

Windows trailing-dot/space and ambiguous unresolved aliases fail closed near protected identities. Cooperative validation is not a guarantee against concurrent administrative path replacement or an unqualified network filesystem.

### Bounded verification and safe archives

Real process fixtures cover ordinary success/failure, noisy output, deadline expiration, held pipes and redirected background children. Canonical commands use a default **64 MiB output limit**, configurable from 1 byte through 512 MiB; exceeding it fails the gate. The overflow observation is bounded, and an explanatory retained-log trailer is bounded by 2,048 bytes. Execution and EOF share a finite deadline, cleanup waits are bounded, and POSIX success requires the ordinary owned process group to be absent. Windows proves leader exit and output closure with narrower descendant certainty; deliberately detached children remain outside the owned-group claim.

Current 0.24 acceptance requires complete ordered startup/gate execution metadata and exact logs. Partial, failed, incomplete or reordered receipts cannot authorize desktop publication. Archive checking bounds ZIP metadata, member namespace/types and expanded size before private extraction, preserves modes, runs passive verification before optional bounded help startup, and rereads the tree/archive afterwards. Both configured CI platforms run the extracted-package check before upload and retain failure reports. Startup smoke is not a native-window or publisher-authentication claim.

## Fresh native journey and final confirmation

The [native record](verification/v0.24-native/README.md) separates the exact delivered 0.23 baseline, candidate work, final executable confirmations and withdrawn experiment. Actual Linux X11 windows used software Vulkan at **1440×900** and **1100×700 with saved 150% text**. Every accepted capture has original PNG bytes, hash/size/dimensions and the window PID observed before and after.

### Connected candidate journey

The candidate accepted the four-wood Riverside supply request, travelled and delivered, reaching **256.13 credits**, **40 XP** and **one completed contract**. It approached the named Industry facility and executed Pause/Resume. It claimed a plot, crafted planks, built fence #2, cancelled named damage with the keyboard and then separately confirmed damage from 40 to 15 health.

It then approached the garage with actual RTS movement, registered **Field Service**, deposited **50.00 credits**, hired **Mira**, saved **Town service** with stops 2 and 3 for one passenger, assigned the **Trail utility pickup**, started service and paused it safely. It used F5 and ordinary Save and quit. All 80 candidate captures remain retained. Company “Review setup” opens Fleet and still requires manual garage navigation; it is not an automatic travel feature.

The candidate process exited 0 with unchanged implementation inputs. Its executable preceded the final two short damage labels. The baseline process also exited 0; incomplete PNG 02 and the baseline helper's original release-label error remain rejected/corrected evidence, not final provenance.

### Final exact executable

**`confirmation-building` PID 76362** restored the real building checkpoint through F9, verified the shortened labels and complete named destruction/no-refund review at 1100×700/150%, cancelled safely and retained fence health 15/40. It saved/quit with exit 0 and unchanged inputs.

**`confirmation-final-fleet` PID 79904** restored the actual final company checkpoint through F9, verified progression, cash, Mira's paused state, vehicle/route assignment and explicit departure/payroll information, then saved/quit with exit 0 and unchanged inputs. It finished at **13:13:44.633927 UTC**, before canonical verification started at **13:13:54.011453 UTC**.

Both processes use the exact final executable:

`509d831c5c6ff1c6619a6ed7fc9feb92ed24dbd737048200f5aca6473ed147e6`.

Both bind the same 390-input canonical map digest:

`37d6c0e5a9e5170959980c86134501c1693a270d6c375253fa94bf642c834aaf`.

The [explicit registry](verification/v0.24-native/APPROVED-REGISTRY.json) selects **18 individually inspected images**. The [accepted provenance audit](verification/v0.24-native/confirmation-provenance-accepted.json) **passed and authorizes release**: it validates both process lifetimes, unchanged source, executable identity, every selected capture and the actual successful terminal canonical receipt. The earlier pending audit remains retained with its original non-authorizing status.

### Corrected visual assessment

An initial full-frame assessment incorrectly reported missing text. Independent exact RGB comparisons proved the named-review area in 208/209 and Build prose in 219/220 were identical and present; all compared regions had zero differing pixels. [The comparison](verification/v0.24-review/pixel-comparison.md) and explicit reviewer withdrawals are retained. The experimental rendering system and test were removed exactly, restoring all 390 input hashes and the original final binary. No renderer fix is shipped or claimed, and its experimental test is excluded from final totals. The earlier Fleet run that observed source changes is also excluded from final acceptance.

### Independent saved-state readback

[Candidate](verification/v0.24-native/candidate-checkpoint-audit.md), [restored copies](verification/v0.24-native/confirmation-checkpoint-audit.md) and [fresh final Fleet](verification/v0.24-native/final-checkpoint-audit.md) audits retain selected value comparisons, file hashes and process associations. They are evidence checks, not additional Rust test executions.

| Persisted value | Building checkpoint / restored building | Final candidate / final restored Fleet |
|---|---:|---:|
| Personal credits | 256.13 | 186.13 |
| Community XP / completed supply contracts | 40 / 1 | 40 / 1 |
| Inventory | 24 wood, 2 planks, 20 stone, 8 wheat seeds | Same |
| Fence #2 | 15/40 health | Same entire structure record |
| Field Service cash | No company yet | 39.34 |
| Mira / Town service | Not created yet | Paused, no loaded trip, same pickup/route assignment |

Company money reconciles **50.00 deposit − 10.00 recruitment − 0.66 payroll = 39.34**; the personal change reflects **20.00 registration + 50.00 deposit**. World time/needs advanced normally, so whole-file equality after resumed play is not asserted. Full private save payloads are not included in the source artifact.

## Review, preservation and delivery

The [review](PRODUCT_REVIEW.md), [research](RESEARCH_v0.24.md), [plan](IMPLEMENTATION_PLAN_v0.24.md) and specialist records cover 20 product perspectives and retain all G01–G18. Independent cross-review corrected rental overbinding, receipt admission, Windows alias ambiguity, preservation-after-checkpoint risks and overbroad Build wording.

Every regular member of the exact **3,092-file 0.23 source archive** is subject to explicit preservation policy. Original assets/data/examples/licenses/historical evidence stay intact, the prior current reports remain exact archived copies, and the old root manifest is retained at a named historical path. The final source packer rereads all source/baseline bytes and modes, actual acceptance evidence and the native executable, then verifies the full ZIP twice. It refused missing terminal acceptance in the retained negative test.

The separate [delivery record](verification/v0.24-delivery/README.md) records the final desktop's actual fresh extraction, passive verification and bounded help smoke. The source archive's external verification JSON binds its final byte identity without a circular self-hash. Those artifact receipts, native rendered play and canonical implementation acceptance are separate evidence that must agree on executable identity.

## Remaining qualification

These are developer-operated finite scenarios on software Linux rendering. Actual Windows/clean-machine execution, physical GPU/controller/wheel/assistive qualification, uncoached sustained careers, complete 200% reflow, physical frame/storage budgets, populated-community policy, licensed production content, migrations, signed upgrades, public-service controls, WAN capacity and offsite recovery retain concrete closure criteria in [the full gap register](REVIEW_GAPS_v0.24.md).
