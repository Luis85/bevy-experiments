# Validation and release evidence — 0.17

**Automated and native acceptance passed.** All **43 final gates** passed on
**367 unchanged implementation inputs**, recording **928 distinct Rust tests**,
**1,014 passing test executions**, **zero failed and zero ignored**.
Five direct native confirmations then used the exact accepted executable and
exited 0. Review date: **2026-10-29**; evidence timestamps are UTC.

The [release record](release-verification-v0.17.json) binds
[final gates](verification/v0.17-final/gates.json),
[native acceptance](native-v17-verification.json),
[extracted desktop acceptance](desktop-package-v0.17.json) and
[preservation](verification/v0.17-final/baseline-comparison.json).
The accepted [0.16 validation](VALIDATION_v0.16.md) remains unchanged.
The [product review](PRODUCT_REVIEW.md) explains the experience; the
[gap register](REVIEW_GAPS_v0.17.md) retains wider G01–G18 closure criteria.

## Reproduce the gates

```sh
python3 scripts/verify.py --offline --jobs 3 --report-dir docs/verification/v0.17-final
```

Use a new report directory for another run; accepted records are retained.
The pinned compiler is **Rust 1.99.1**, using locked dependencies. The runner
records commands, outcomes, logs, published report hashes, actual distinct test
identities and implementation hashes. Repeated feature/report executions are
not counted as different tests.

The gate runner is unchanged from accepted 0.16. All 43 inherited gates remain:
compatibility/formatting; headless/domain tests; three priced careers; original
queue recovery; navigation clusters and geometry; continuing guidance; Bevy
headless/render integration; native integration; launcher recovery/world naming;
examples; original wildlife, vehicle and character generation/inspection/
publication; prepared source distribution; recovery probes; real TLS regional
sessions; and the final native build. The final run completed at
**2026-10-29 09:40:55 UTC**. A separate reviewer rederived commands and test
identities from the actual logs.

| Crate | Distinct passing Rust tests |
|---|---:|
| terra-bevy | 147 |
| terra-cli | 4 |
| terra-core | 33 |
| terra-demo | 181 |
| terra-history | 13 |
| terra-net | 121 |
| terra-persistence | 38 |
| terra-sandbox | 346 |
| terra-stream | 45 |
| **Total** | **928** |

Accepted executable SHA-256:
`a6a6b6c1fd840aa448a73e6925b0d2e79ee2d8602fb47ef3af169d7eb44a0d44`.

Final gate record SHA-256:
`539c17efce1d3cf5a452561a1cda5732d34216c3199015699a6bd30208ec2cf4`.

Save 1, regional schema 1, wire 10 / ALPN `terra-world/10` and asset schema 1
remain unchanged. Optional human/seat asset fields retain the 0.16 reader
requirement. The new optional launcher preference does not alter world saves;
downgrading to an older preference reader can reset graphics defaults with a
warning. See [Compatibility](COMPATIBILITY.md) and [Playtesting](PLAYTESTING.md).

## New reliability and interaction regressions

The [worker review](verification/v0.17-review/reliability-implementation.md)
records actual lost-checkpoint completion, lost hydration completion and idle
request-channel loss. A committed generation 2 survives the lost acknowledgement
with its complete archive and metadata; validated reopen then saves generation
3. False pending is cleared, but submission and barriers stay terminal until
reopen. Ordinary SQLite rollback/retry remains distinct. All 38 persistence
tests pass, including existing crash/corruption/backup coverage.

The [journey review](verification/v0.17-review/player-journeys.md) records
assignment-aware Interact, rebound-key help, driving status, full rendered
rental-return context, one-shot review and stale queued-action rejection.
Terminal local Close failed world is guarded by actual terminal status, local
ownership and loading/session screen context. Normal portable save, restore,
write failure, retry and quit are covered separately. The integrated final
terra-demo gate passes **181 tests**.

The [diagnostic review](verification/v0.17-review/diagnostics-implementation.md)
covers persisted default-Off choice, preference preservation, bounded/cached
summary, publication-derived status, paused/failed/save state, exclusive writer
ownership, actual killed-process lock release, concurrent writers, aliases,
unrelated destination refusal and final-acknowledgement ordering. Privacy tests
use private-capability and typed-text sentinels and absent inventory/player
payload keys. Balance/raw-log omission is source-reviewed; it is not presented
as a separate executed sentinel test.

These targeted regressions test real state transitions and failure contracts.
They do not create a production fault-injection interface, time human travel,
or establish long career balance. Cross-review corrections are retained in the
[0.17 review folder](verification/v0.17-review/). Development integration logs
remain development evidence; the complete final gates supersede them.

## Retained guidance, economy and navigation gates

The three career gates rerun existing funded strategies, including bought and
novice paths. The [continuing-goals report](verification/v0.17-final/guidance-continuity.json)
again accepts three requests, returns one, expires one and physically completes
one, checking exact resumed continuation and ownership/value rules. Its declared
calendar advancement is separate from physical walking time. Finite test
strategies do not price every living/labor/capital cost or prove career balance.

[Traffic recovery](verification/v0.17-final/traffic-recovery.json),
[cluster navigation](verification/v0.17-final/navigation-cluster.json) and
[geometry](verification/v0.17-final/navigation-geometry.json) retain bounded
physical movement, support/body-sweep checks, original work and destination,
value conservation and exact continuation. The inherited five geometry cases
cover a narrow parallel parked body, temporarily unavailable/restored flank,
unavailable reconnection, moving crossings and a blocked narrow junction.

Additional corridor admissions remain capped at 12 per vehicle update.
The legacy reconnection source check rejects beyond 160 m before querying
support; complete admitted reconnection is divided into bounded calls. New
narrow/cluster routes admit complete support and sweeps. Legacy future traffic
occupancy is checked during physical stepping, retaining established loaded
courier behavior. These are authored finite flat geometries. The support-loss
case includes safe movement, and the blocked junction needs an explicit blocker
move. They do not establish general dense-traffic or intersection throughput.
The precise reports and [historical navigation review](verification/v0.16-review/navigation-implementation.md)
retain those limits.

## Assets and prepared sources

Original wildlife, Comet and Fieldhand/cabin generators, inspections, recipes,
immutable publication and authored validation all execute again. All protected
original content inputs remain unchanged. Hierarchy, named clips, all animation
channels including unused clips, scales, material roles, fallback readiness,
pause/blend, per-binding policy, allocation budgets, known local occupancy and
first-person visibility retain their regression coverage.

The seven-model direct composed pack identity is
`sha256:124a1baa698dce14cbdbb39e8896673611059af098580c19dc4cfc694e6cd2c0`.
It contains the original five wildlife models plus Fieldhand and the fitted
Comet cabin. Recipe-built packs can have different identities because their
names and content-addressed paths differ; each build/validation pair must agree.
Production retargeting, facial/equipment/passenger work, audio and target-device
residency are not established by this example.

Prepared-source inspect/export/install/verify rerun against the connected
heightmap bundle identity:
`heightmap-set-v1:1100816335f7077227658413231028310009b25af7c8f5797e578b088cd151a3`.
Seam, coverage, provenance and normalized persistence-ID contracts remain.
Licensed shipping data, NoData/datum decisions and complex road topology remain
separate product work.

## Final native confirmation

The [native receipt](native-v17-verification.json) binds **five** completed
direct processes and **14 observed image roles** to the exact final executable,
post-gate times and unchanged source map. Every image has a real native capture
receipt with matching SHA-256, dimensions and stable window PID before/after.
The [final visual review](verification/v0.17-review/final-native-review.md)
explains actual actions and observations.

| Accepted process | Actual scope | Final measured outcome |
|---|---|---|
| Named world play/save | Managed Playtest 017, movement, pause, F12 and keyboard Save and quit | 49 chunks, 0 terrain pending, checkpoint 68, no unsaved state |
| Same profile reopen | Separate process, actual regional restore, pause and Save and quit | 49 chunks, checkpoint 85, no unsaved state |
| Paid Comet driver | Explicit F9 portable restore, W/P/K/C, seven-model pack, Save and quit | 9 prepared chunks; 48 ready imports and two intentionally hidden actors in first person |
| Larger preset | New procedural regional world, F12/pause and Save and quit | 121 chunks; 107 bodies / 64 wildlife; checkpoint 68 with 121 active and 11 dormant regions |
| Main menu | Settings/report details, Home and Quit | Retained reopen session, capture 7 and checkpoint 85 displayed with stale-health caveat |

The [save/reopen comparison](verification/v0.17-native/save-reopen-comparison.json)
uses private read-only SQLite/profile copies after the first two clean exits.
Both contain 56 region rows. Identity, catalog, player, inventory hash, personal
money (25,000 minor units), avatar ID/owner/name, character choices and saved
clock pause/time-scale settings match. Generation advances 68 → 85. Simulation
time, needs and environmental state can advance; whole-world equality is not
claimed. The independent audit rederives these fields from the private copies.
The copies themselves are excluded from delivery.

Each accepted game stream has a separate session/PID and at most 16 retained
samples. Earlier startup loading, ordinary pending work and bounded fallback
are valid states. Final reports have settled terrain, no pending/blocked
checkpoint, no unsaved state, and no world/simulation/storage/model/export error.
The final `graceful_exit` sample observes an exit request with phase `saving`;
separate process records establish exit 0. Diagnostic success is not substituted
for save durability.

At 1100 × 700 and 150% text, the actual keyboard flow reveals and focuses Save
and quit. Pause and the unobscured F12 bottom show the full report path.
The diagnostic reviewer independently inspected both pause sizes and that F12
capture. The journey reviewer independently inspected both driver views.
No further material UI correction was requested after the freeze.

Candidate menu creation, diagnostic opt-in and save/return are retained in the
[candidate manifest](verification/v0.17-native/candidate-evidence-manifest.json),
separately from final confirmations. Initial restoration of the paid fixture
requires the ordinary F9 portable-save action; merely supplying its config does
not load the JSON save. That fixture has no accepted transport assignment.
No native completed delivery, rental return/refund or worker-loss fault injection
is inferred.

The native environment is Linux x86_64, X11/Xvfb and software Vulkan. Captured
sizes are 1440 × 900 and 1100 × 700 with 150% text. These expert-operated,
prepared scenarios do not qualify a Windows PC, clean-machine installation,
physical GPU/controller/wheel, assistive access, uncoached usability, dense
population, general traffic or hardware frame-time targets.

## Larger regional storage and local TLS

The [regional-session workload](verification/v0.17-final/regional-sessions.json)
uses 16 real clients, eight churn cycles, 400 active and 200 offline ticks per
cycle, and 1,024 cold overlay regions. Including the source-loss/recovery phases,
it executes 4,840 fixed steps. It checks actual snapshots, offline/resume/restart, durable
acknowledgements, cargo/crafting/sequence conservation, complete backup private
metadata/source bytes, unavailable/restored source and failure-atomic retry.
Current sample counts, measured timings and store statistics stay in that report.

The automated 1,024-cold-region case is separate from the 121 rendered chunks in
the native larger world. Local TLS, finite churn and a shared software-rendering
host do not establish public Internet capacity, long-soak stability, a supported
population or deployment recovery objectives. Existing recovery-bundle/probe and
launcher recovery/naming gates remain part of the complete acceptance.

## Delivery and preservation

The desktop packager consumes the exact accepted ELF and **152 explicit project
payload files**, preserving all 148 prior payload paths and adding the current
playtest/report/history handoff. It also includes the final automated evidence,
launcher and package manifest. The [desktop receipt](desktop-package-v0.17.json)
is the authority for completed package acceptance, archive identity and build
profile.

The extracted launcher and prepared seven-model world are run from an unrelated
working directory. Retained file traces bind actual execution to the extracted
binary, confirm successful packaged-source/model opens and reject current or
prior checkout references. The exact payload hashes/modes are compared before
and after both runs. Runtime dependency resolution is qualified only on this
Linux host; system graphics/input/C runtime libraries are not bundled.

The source preservation contract retains all **1,488 prior paths**, **102
protected original data/asset inputs** and **1,068 historical evidence files**.
The former current Product Review and Validation are archived exactly as 0.16.
The final source assembly validates its full member inventory, hashes, CRC,
canonical paths, regular-file modes, implementation freeze and exclusion of
private saves/databases/identities. Its archive checksum is recorded outside the
archive to avoid self-reference.

An independent archive audit reconstructs these checks from both finished ZIPs
and the retained tested desktop extraction; it also rederives final commands,
test identities, process/capture/telemetry provenance and private save/reopen
summaries. Its external result is linked from the delivered standalone report.
This guide is finalized before desktop assembly, so the source and packaged
guide bytes can be compared unchanged. Completed receipts, not planned checks,
determine whether delivery is accepted.

The complete source archive contains current code, reports, screenshots, logs
and history. The smaller Linux review archive contains its executable, reviewed
player/content payload and automated evidence. Source/developer/historical links
refer to the companion source archive. Neither deliverable contains the new
private player save, regional database or resume identity.
