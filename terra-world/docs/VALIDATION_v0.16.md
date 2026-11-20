# Validation and release evidence — 0.16

**Automated and native acceptance passed.** All **43 final gates** passed on
**366 unchanged implementation inputs**, recording **904 distinct Rust
tests**, **990 passing test executions**, **zero failed and zero ignored**.
Four native confirmations then used the exact accepted executable and exited 0.
The [release record](release-verification-v0.16.json) binds final gates,
[native evidence](native-v16-verification.json),
[extracted Linux package evidence](desktop-package-v0.16.json) and
[preservation](verification/v0.16-final/baseline-comparison.json).
Review date: **2026-10-29**; recorded process timestamps are UTC. The accepted
[0.15 validation](VALIDATION_v0.15.md) remains byte-for-byte unchanged.

The scope is a regional SDK and reference game. The [product review](PRODUCT_REVIEW.md)
explains the observed experience and correction batch. The
[open-gap report](REVIEW_GAPS_v0.16.md) preserves the broader G01–G18 criteria;
finite fixtures and local expert review do not close those wider areas.

## Reproduce the gates

```sh
python3 scripts/verify.py --offline --jobs 3 --report-dir docs/verification/v0.16-final
```

Use a new report directory for another run: accepted evidence is retained.
The pinned compiler is **Rust 1.99.1** with locked dependencies. The runner records
commands, statuses, test identities, logs, published report hashes and all
implementation hashes. It distinguishes unique test identities from repeated
executions under different feature configurations and selected report gates.

The [complete gate record](verification/v0.16-final/gates.json) is the authority
for counts and completion. Its 43 gates cover compatibility/formatting,
headless/domain tests, three priced careers, original queue recovery, stopped-body
clusters and the new geometry matrix, continuing commitments, Bevy headless and
rendered integration, native integration, recovery/naming, examples, original
wildlife and vehicle assets, character generation/inspection/publication,
prepared source distribution, local recovery probes, real TLS session churn and
the final native build. No gate was removed to make this release pass.

| Crate | Distinct passing Rust tests |
|---|---:|
| terra-bevy | 147 |
| terra-cli | 4 |
| terra-core | 33 |
| terra-demo | 160 |
| terra-history | 13 |
| terra-net | 121 |
| terra-persistence | 35 |
| terra-sandbox | 346 |
| terra-stream | 45 |
| **Total** | **904** |

Accepted executable SHA-256:
`6ba842ceaa3a9efe88603ae81db48053606185655827b3b5cf657dbe5c296434`.
Native, desktop and release receipts use that exact identity. Gameplay save 1,
regional schema 1, wire 10 and ALPN `terra-world/10` remain unchanged. The new
optional human and driver-seat fields require a 0.16 asset reader; absent fields
preserve older normalized pack identities. See [Compatibility](COMPATIBILITY.md).

## Navigation and continuity measurements

The [geometry report](verification/v0.16-final/navigation-geometry.json) contains
five named cases: parallel parked body on narrow support, temporarily unavailable
then restored flank, unavailable reconnection, two moving crossings and a blocked
narrow junction. It retains actual physical times/distances, bounded step/heading,
source-query work, path limits, value conservation and exact continuation flags.
The [independent implementation review](verification/v0.16-review/navigation-implementation.md)
and [0.15 baseline](verification/v0.16-review/navigation-baseline-015.json)
explain the original oscillation and the changed route admission.

These are authored flat SDK geometries with a frozen calendar. Additional
corridor admissions are capped at 12 per vehicle update; the reconnection source
check rejects beyond 160 m before invoking support. The complete legacy
reconnection is divided into calls bounded by 64 m of translation plus rotational
envelope travel, with 0.1 m numerical/support tolerance, each charged to the same
12-call allowance. Every segment must pass before commit. The
[career regression review](verification/v0.16-review/navigation-career-regression.md)
explains the existing per-callback limit and the final-segment failure regression.
New narrow/cluster proposals
admit complete support and sweeps. The legacy single-body future occupancy
remains checked during physical stepping, preserving established courier cases.
The 600-second unsupported interval includes repeated safe clearances, not a
stationary or unpriced operating-cost claim. The blocked junction resumes after
an explicit Move order physically clears the blocker. No general intersection,
dense-traffic or target-hardware throughput claim follows.

The [continuation report](verification/v0.16-final/guidance-continuity.json)
records three accepted requests, one return, one expiry and one physical delivery.
It checks exact resumed steps, retained obligations/ownership and ordinary payment.
Its 86,401 seconds of calendar advancement are distinct from 70.8 physical seconds
of walking. The path covers about 315.9 m with bounded steps. The delivered four
wood earn 568 minor currency units (5.68 credits) and 40 XP, changing the wallet
from 250.00 to 255.68 credits (25,000 to 25,568 minor units). These are explicit
test inputs/results, not a full day of
human play or a balance conclusion.

The three existing career measurements, the cluster route and both original
loaded-courier recoveries execute again. Their reports remain finite managed
strategies. The passenger strategy uses five explicit hold/resume interventions;
living/management costs remain unpriced, public funding finite and ecology time
separate. The original rear-trap and occupied-exit cases retain physical return,
accepted obligations and conserved values. Historical benchmark values are not
silently replaced by current host timings.

## Character and asset-production checks

The generator reproduces five outputs: Fieldhand GLB, fitted cabin GLB and three
JSON declarations/recipe files. Inspection and validation cover named clips,
full hierarchy, all clip channels including unused clips, positive scales,
material-role uniqueness/use and admission budgets. The seven-model recipe is
built into an immutable content-identified pack and that published output is
validated again. Direct and recipe-built pack identities may differ because
names and content-addressed paths differ; each build/validation pair must agree.

| Original new asset | Bytes | SHA-256 |
|---|---:|---|
| Fieldhand | 162,140 | `b5d62261ee73a9a987b754b322ea95ce9d84941ca82e03c87de00568aa759b7e` |
| Fitted Comet cabin | 224,424 | `07f58f28cc002288213bec5a03db3e318fb07ec8f6708aa713fd807332353d17` |

The standalone prior Comet and all five wildlife models remain unchanged.
The direct composed runtime pack identity is
`sha256:124a1baa698dce14cbdbb39e8896673611059af098580c19dc4cfc694e6cd2c0`.
Three private tint handles per human preserve the existing sRGB palette; teardown,
profile reuse, fallback readiness, pause/blend, known manual occupancy and
first-person visibility have regression coverage. The 12 KiB material allowance
and scene admission totals are estimates, not measured GPU residency.

Independent actual-GLB geometry review sampled 49 seated times across all three
styles and corrected roof, floor, shell and hand positioning. It records authored
clearance, not IK, passenger handling or animated physical boarding. Native
appearance supplies a separate visual check.

## Managed recovery and world naming

The [world-name report](verification/v0.16-final/launcher-world-names.json) covers
two recovered copies with deliberately equal timestamps, persistent distinct
names, title-only rename, exact prior-tree preservation and fresh relisting.
Additional native tests cover stale/queued/cancelled actions, invalid names,
external paths, hardlink/symlink aliases, edited queued recovery names and no
automatic opening. Unplayed-world rename does not create a checkpoint.

The [launcher recovery report](verification/v0.16-final/launcher-recovery.json)
retains the original-loss, source/catalog/politics, regional generation and
recovery identity checks. Naming uses existing profile metadata and does not
change the save schema or introduce content migration.

## Native confirmation on the final executable

The [native acceptance record](native-v16-verification.json) binds four clean
processes after the final gates: paid Comet/character interaction, the continuing
commitments interface, named recovery review and the recovered regional world.
Each process records the accepted binary hash, unchanged implementation inputs,
start/end times, log hash and exit 0. Window captures are real native captures
at 1440 × 900 and 1100 × 700, including 150% text.

The final driving review checks concise configured help, remaining-route units,
Exit keeping the nearby Comet selected and re-entry. Third person and first
person show the seven-model pack without failed models or rejected instances;
first person deliberately hides two actors (rider and exterior). Hidden actors
may contribute to fallback accounting and must not be counted as failed imports.
Global telemetry does not export per-body pose, tint, seat or panel selection;
those findings use screenshots, state/regression checks and explicit observation.

The final guidance review confirms the concise gross reward/full missing-quantity
comparison and the exact Review handoff revealing the guide at enlarged text.
A [before/after authority comparison](verification/v0.16-native/v16-confirm-guidance-review-comparison.json)
checks the paused saved state, identity and player without distributing private
save files. Review is not a travel or transaction command.

The new “Valley - final review” copy was compared before opening. Every SQLite
schema definition, column and typed value matched the selected backup: 19 region
rows and one world row. Character, quality, start hour, source/political/catalog
bytes, identities and rebased runtime paths matched. All **57 pre-existing
fixture files** remained unchanged before and after opening. The original world
and source tree remained absent. The reopened world reached 49 ready chunks,
reported regional restoration and exited without world, simulation, persistence
or asset errors. Final scene telemetry records 64 ready imported instances at
its 64-instance cap, nine budget-deferred and 18 distant actors retaining
procedural fallback (27 total), with no loading or unmetered instances and no
draining. All selected/reserved work stays within the declared limits. These
larger-scene counts are separate from the driver's 50/48 visibility check.
See [exact recovery](verification/v0.16-native/v16-confirm-recovery-exact.json)
and the [final native review](verification/v0.16-review/final-native-review.md).

The environment is Linux x86_64, X11/Xvfb and Mesa llvmpipe software Vulkan.
There is no physical GPU, Windows, controller or steering-wheel qualification.
Prepared later-state fixtures are agent-operated, not uncoached sessions.
Settling delays and input holds on this shared host are not performance results.

## Local TLS and larger regional operation

The [session workload](verification/v0.16-final/regional-sessions.json) uses 16
real clients, eight churn cycles, 400 requested ticks and 1,024 cold overlay
regions. The fixed workload executes 4,840 steps and retains 4,096 cold resources.
It checks real snapshots, offline/resume/restart, durable command acknowledgements,
cargo/crafting/sequence conservation, complete backup private metadata and source
bytes, unavailable/restored source and failure-atomic retry. The report retains
sample counts, observed latency distributions and final store statistics.

The native recovery sample remains 19 saved regions; it is not presented as the
1,024-region UI case. Local TCP/TLS, finite churn and a shared software-rendering
host do not establish Internet capacity, long-soak stability, a supported player
population or deployment RPO/RTO. Those criteria remain G13–G18.

## Earlier attempts and corrected findings

The [first integrated native test log](verification/v0.16-review/demo-integration-attempt-1.log)
records 157 passes and one failed palette assertion. Imported materials had
interpreted sRGB palette values as linear. Correcting the production conversion
and asserting actual procedural parity produced 158/158 in the next development
integration run. The final gates supersede that development result.

The [first release attempt](verification/v0.16-attempt-1/gates.json) stopped at
headless compilation because two new regression fixtures called nonexistent
`LocalFrame::unproject`. Only those fixtures were corrected to
`to_geo(...).unwrap()`. All nine focused guidance tests then passed, and the full
43-gate run restarted in a fresh acceptance directory. The next
[attempt](verification/v0.16-attempt-2/gates.json) exposed a real navigation
regression: two established warehouse careers stalled after six deliveries.
The navigation owner reproduced and corrected the production route admission;
its focused regression report records the cause and preserved career checks.
The final complete gates supersede both attempts. Failed records remain
available; no gate or career assertion was weakened.

One exploratory native setup omitted `--no-default-politics` for a politics-free
save. F9 correctly rejected its identity. A window-close left its process alive;
the owned process was stopped with SIGTERM (exit -15). It is explicitly excluded
from acceptance. The corrected candidate command and all final confirmations
completed normally. Input-helper retries and modifier timing during exploratory
review are also not converted into product/performance claims.

## Delivery, preservation and evidence boundaries

The Linux review ZIP is assembled from this exact accepted executable and
148 explicit project payload files plus current gate evidence and generated
launch/manifest files. The extracted launcher and prepared character-pack world
are checked from an unrelated working directory, with file traces checking
checkout independence, local runtime dependencies and unchanged package payloads.
The [desktop receipt](desktop-package-v0.16.json) records completed outcomes,
archive hash, member count and the precise build profile. It is authoritative for
package acceptance; this guide is finalized before package creation so its bytes
remain identical in source and desktop delivery.

The source archive preserves all **1,213** prior paths, **96** original protected
inputs and **804** historical evidence files. Earlier current review/validation
pages are archived exactly. The final assembly verifies the complete source
snapshot, archive CRC, every member hash, executable modes, bounded canonical
paths and exclusion of private saves/databases/identities. The source archive's
hash is recorded outside itself to avoid a self-referential checksum.

The complete source ZIP contains current reports, native evidence, screenshots,
logs and history. The smaller desktop ZIP contains current player guides and its
accepted automated evidence; source/developer/historical links point into the
companion source project. No new private world database or resume identity is
part of either deliverable.
