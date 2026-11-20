# Validation and release evidence — 0.13

Review date: **2026-10-28**. **All 22 frozen-source release gates and the final
native confirmation passed.** The exact accepted results are in
[gates.json](verification/v0.13-final/gates.json),
[native verification](native-v13-verification.json) and the
[release record](release-verification-v0.13.json). The original
[0.12 validation](VALIDATION_v0.12.md) remains unchanged historical evidence.

## Accepted source and executable

| Contract | Accepted value |
|---|---|
| Release | 0.13.0 |
| Wire protocol / TLS ALPN | 10 / `terra-world/10` |
| Domain save / regional schema | 1 / 1 |
| Compiler | `rustc 1.99.1 (58cf8f9c3 2026-10-13)` |
| Frozen source and support inputs | 338 |
| Native executable SHA-256 | `2c9fafc225c5e817785a6f6ef9f9ec087d0dd3c0b597fc722b2327c8bd6b49d0` |
| Build profile | Optimized dev profile (`opt-level = 1`) |
| Automated environment | Linux x86-64, cached dependencies, three build jobs |
| Native environment | Xvfb, Vulkan llvmpipe, Performance quality |

The manifest covers workspace manifests, Rust sources, embedded fixtures,
original model and terrain data, configuration, verification scripts and CI
configuration. It excludes changing reports and prose documentation; the release
record separately hashes the final review, gap report and validation. Every
source input remained unchanged through gates, replay and native confirmation.
All published gate logs and reports retained their captured hashes.

The [baseline comparison](verification/v0.13-final/baseline-comparison.json)
checks the exact 0.12 ZIP, preserves its 733 paths, and confirms the 24 original
terrain, traffic and asset inputs remain byte-identical. The two historical
current documents were archived exactly. The original open-gaps PDF is retained
unchanged in [reports](reports/Terra-World-0.12-Open-Gaps-Report.pdf).

## Complete automated acceptance

There were **785 distinct passing Rust tests** and
**842 passing test executions** across all gates, with **zero failures
and zero ignored tests**. Targeted report gates repeat some test identities;
they are not counted again as distinct tests. The retained
[identity list](verification/v0.13-final/distinct-tests.json) binds crate,
harness and case names.

| Crate | Distinct passing tests |
|---|---:|
| `terra-bevy` | 107 |
| `terra-cli` | 4 |
| `terra-core` | 33 |
| `terra-demo` | 117 |
| `terra-history` | 13 |
| `terra-net` | 121 |
| `terra-persistence` | 35 |
| `terra-sandbox` | 311 |
| `terra-stream` | 44 |

All commands were executed during this release run. Cached compilation does not
replace test execution. The exact per-gate command, status, timing and log hash
are retained in the gate record.

| Gate | Passing test executions | Elapsed seconds |
|---|---:|---:|
| compatibility | 0 | 0.081 |
| format | 0 | 1.860 |
| headless | 560 | 384.383 |
| career-report | 1 | 19.265 |
| purchased-career-report | 1 | 3.097 |
| novice-career-report | 1 | 34.129 |
| traffic-recovery | 3 | 16.885 |
| bevy-headless | 73 | 273.017 |
| bevy-render | 85 | 67.583 |
| demo | 117 | 0.791 |
| demo-check | 0 | 13.206 |
| headless-example | 0 | 0.450 |
| custom-provider-example | 0 | 0.467 |
| asset-pack | 0 | 0.242 |
| source-inspect | 0 | 9.673 |
| source-export | 0 | 0.155 |
| source-install | 0 | 0.152 |
| source-verify | 0 | 0.157 |
| recovery-probe | 0 | 10.234 |
| session-probe-checks | 1 | 0.584 |
| regional-sessions | 0 | 136.016 |
| native-build | 0 | 0.284 |

The run used:

```sh
python3 scripts/verify.py --offline --jobs 3 --report-dir docs/verification/v0.13-final
```

A subsequent run must use a new evidence directory. `--offline` requires cached
Cargo dependencies. The TCP/TLS cases need permission to bind localhost; no
public server or Internet service is implied. The verifier checks the actual
compiler against the pinned toolchain before starting its gates.

## Commercial and movement measurements

The [novice passenger report](verification/v0.13-final/novice-career-report.json)
completed 65 physical deliveries over 3,600 service seconds, then completed its
last commitment and returned the rental by 3,649.5 seconds. The owner began with
250.00 credits, deposited 100.00 in the company and made no later owner deposit.
A rented compact supplied the three-passenger service; the starter vehicle's
fuel, condition and mileage remained unchanged.

| Career cash observation | Credits |
|---|---:|
| Public fares | 821.60 |
| Registration | 20.00 |
| Recruitment | 10.00 |
| Payroll | 70.71 |
| Initial lease and 12 renewals | 23.40 |
| Fuel payment net of return refund | 2.38 |
| Rental return damage | 0.63 |
| Total measured costs | 127.12 |
| Combined owner/company cash gain | **694.48** |

The fixture conserves 13,650.00 credits including escrow and all 160 passengers.
It checks six exact JSON resumes at ten-minute intervals and an exact regional
accounting roundtrip. Five explicit ten-second wait/resume interventions are
recorded, so the result establishes managed service. Ecology time is zero;
owner living costs and management time remain unpriced, and the issuer's finite
funding is not replenished indefinitely. This is a deterministic domain scenario,
not a human playtest. The persisted ledger and external owner/company cash have
different recording coverage, explained in [COMPANIES.md](COMPANIES.md).

Both warehouse counterexamples were rerun unchanged. The
[contributed-stock career](verification/v0.13-final/career-report.json) returns
−84.69 credits after known owner-paid costs, and the
[purchased-stock career](verification/v0.13-final/purchased-career-report.json)
returns −161.49. Their broader unpriced inputs remain disclosed. A viable
passenger case does not erase these losses or prove general balance.

The additional [frozen-source walking replay](verification/v0.13-final/walking-replay.json)
uses the exact generation-251 input and original prepared-source identity.
Its planned route improves from 261.806 m to 75.345 m using 2,817 support queries;
physical arrival takes 18.9 seconds. Every step checks source and body admission,
speed bounds, goods/cash conservation and identical save/resume continuation.
Planning does not move the actor. The private full export is excluded from the
project, while its SHA-256, public geometry regression and replay tool identify
the evidence. This bounded local case is separate from rendered UI observations.

[Traffic recovery](verification/v0.13-final/traffic-recovery.json) reruns the
original stopped-queue saves and their conservation/resume checks. The ordinary
vehicle tests also cover parked rented, exited and company-released vehicles,
whole-group rejection atomicity, missing terrain, fuel, damage and rental expiry.
No vehicle handling, traffic or walking result establishes arbitrary-network
route completeness.

## Community and recipient authority

The domain suite covers leaving, removing members, invitation decline/revocation,
leadership transfer, offline homes and shared-property preservation. The native
suite covers displayed invitation identity, retained reviews and control focus.
The actual two-principal TLS test covers invitation, membership, leadership,
departure, private views, stale command sequences and host save/restart. Its
replica regression admits only rosters relevant through membership, invitation
or ownership of a visible plot; unrelated rosters still reject atomically.

Protocol 10 reflects the added command semantics. Compatible domain and regional
saves remain version 1. Strict source/catalog identity checks remain in force;
this release does not supply a source-data migration.

## Native review and final confirmation

Two candidate sessions reviewed route creation and communities. Three final
sessions used the exact gate-built binary: company confirmation, company reopen,
and community restore. All five processes exited cleanly. The retained record
separates their timing and phase and includes the inspected screenshots.

Actual observations include the three-passenger picker and quoted load receipt,
readable scrolling at 1100×700/150% text, the six Fleet destinations, keyboard
Save and Quit, the Controls text preference, named leadership consequences,
Tab-to-Cancel without mutation, and an accepted handover restored through F5/F9.
The final company journey reopened the saved three-passenger route and financial
state. Exact comparisons cover company/accounting, owner account, player record
and inventory, avatar, character profile, progression, territory, vehicle,
world identity and politics. The community handover changed only the expected
leadership/role fields; its complete domain state survived final restoration.
Private complete worlds and resume capabilities are not published.

All retained native telemetry records report no world, simulation or model-pack
error. Each accounts for 64 wildlife presentations as ready models or procedural
fallbacks, within the configured selected/reserved scene bounds. These are
software-rendered functional observations and estimated admission counters,
not target-GPU frame-time or memory qualification. Physical controller/wheel,
assistive-technology and full keyboard-only acceptance remain open.

## Content, recovery and desktop boundary

The asset CLI accepts the five-species immutable wildlife example. The original
pack bytes are preserved. Portable source inspect/export/install/verify gates
exercise the connected prepared map and its exact coverage/identity rules.
Recovery and the combined 16-client TLS workload exercise actual sessions,
prepared terrain, 1,024 dormant overlay regions, eviction/resume and failure
recovery. The retained [recovery report](verification/v0.13-final/recovery.json)
and [regional-session report](verification/v0.13-final/regional-sessions.json)
give exact bounds and checks. These local finite workloads do not establish WAN,
public-account policy, sustained capacity, deployment recovery objectives or MMO
sharding.

The desktop packager accepts only the exact gated executable and explicit
reviewed payload, binds its evidence and compiler, hashes every member and checks
the resulting ZIP. The [desktop verification record](desktop-v13-verification.json)
records Linux extracted-folder startup and prepared-world loading separately.
System graphics/input/C runtime dependencies are not bundled. Windows CI and
packaging are configured but were not executed in this environment; no Windows
binary, signing, installer or clean-machine qualification is claimed.

The complete remaining work is retained under all 18 original identifiers in
[REVIEW_GAPS_v0.13.md](REVIEW_GAPS_v0.13.md). Earlier evidence and incomplete
historical attempts remain labeled by their own release and are not counted in
this accepted run.
