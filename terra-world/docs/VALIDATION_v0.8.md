# Terra World 0.8 validation record

Validation date: **2026-10-27**. Source release **0.8.0**, wire protocol **6**,
TLS ALPN **terra-world/6**, domain save version **1**, regional database schema
**1**. The final gates contain **534 distinct passing Rust tests**, with zero
final failures and zero ignored tests. These are correctness results for the
bounded SDK and reference game.

The [machine-readable release report](release-verification-v0.8.json) links the
automated, native, storage and actual-session evidence. The earlier
[v0.7 validation](VALIDATION_v0.7.md) and its captures remain historical; their
counts and performance measurements are not combined with this release.

## Automated gates

| Crate | Distinct passing tests |
|---|---:|
| `terra-cli` | 4 |
| `terra-core` | 28 |
| `terra-history` | 13 |
| `terra-net` | 100 |
| `terra-persistence` | 32 |
| `terra-sandbox` | 194 |
| `terra-stream` | 20 |
| `terra-bevy` | 69 |
| `terra-demo` | 74 |
| **Total** | **534** |

The [headless report](headless-verification-v0.8.json) records the seven
non-Bevy crates' 391 cases and six empty documentation harnesses. Bevy contributes
48 render-library tests plus 21 integration tests. Its 28 no-default-feature
library cases also pass and overlap the render-library set; they are counted
once. Bevy's documentation harness is empty. Targeted reruns, probes, examples,
native actions and earlier failing/edit-race runs add no extra Rust cases.

Commands use the committed lockfile:

```sh
cargo test --workspace --exclude terra-demo --exclude terra-bevy --locked
cargo test -p terra-bevy --no-default-features --locked
cargo test -p terra-bevy --lib --locked
cargo test -p terra-demo --bin terra-demo --locked
cargo run -p terra-bevy --no-default-features --example headless --locked
cargo run -p terra-bevy --no-default-features --example custom_provider --locked
cargo build -p terra-demo --locked
cargo fmt --all -- --check
```

The recorded environment uses Rust 1.99.1, pinned Bevy 0.20.0, Linux x86_64,
locked cached dependencies and two Cargo jobs. Actual TCP/TLS cases ran with
permission to open localhost sockets. Sandbox-denied attempts are not counted
as passed tests. Final source fingerprints, executable hashes and measurement
provenance distinguish the implemented release from earlier builds.

## What the gates establish

### Commercial lifecycle and conservation

The 24 company and seven rental integration cases exercise finite money, actual
cargo and inventory, wage settlement, assignment, quote admission and loaded
commitments. Hold suspends payroll; physical return refunds the original issuer
without granting a delivery reward; nearby handover preserves the exact load,
escrow, pickup time and admitted travel proof. Dismissal preserves earned money.
Private depots enforce combined inventory capacity and physical reach. Household
spending buys actual goods/transit with existing wages and freezes offline.

Rental tests cover fee/fuel/bond quoting, finite fuel issuance, renewal, expiry,
stopped empty return, capacity/funding/quote rejection and exact save/load.
Expiry retains loaded work. Return cannot delete an active assignment or mint a
refund when the issuer lacks funds. Domain and native-command tests share these
rules rather than reproducing arithmetic in a separate UI mock.

### Roads, traffic and occupied construction

Twelve traffic integration cases exercise directed one-way/access plans,
grade-separated junctions, geographic seams, source speed limits, manual
occupancy, blocked exits, large physical steps, stale regional requests and
bounded FIFO ticket handling. Four approaching vehicles must clear the junction
while preserving contact bounds. Stalled permits requeue; an actually occupied
junction does not become clear because its lease expired. Save/resume preserves
traffic decisions. Reverse-way population spacing and over-budget facility
routes have dedicated regressions; forbidden or oversized plans never become
unlabeled direct shortcuts.

Model-height-aware underpass checks use sampled terrain as the source building's
height datum. Car/bus cases cover clearance and a margin; malformed dimensions
fail conservatively. Manual, host and predicted source sweeps account for
translation and corner travel. The host thin-wall regression starts clear and
crosses a wall between the former one-metre samples.

A whole-game review found that construction used only a small vehicle-center
radius. A new shared oriented-body/column query now gates domain Build and the
native preview. An actual rented bus belonging to one player blocks another
player's overlapping construction on otherwise permitted land, with the entire
world unchanged. Clear side placement succeeds; animal footprints and existing
movement-overlap escape are covered.

### Prediction, admission and regional persistence

Five prediction library tests and an actual TCP owner-acknowledgement case check
bounded outstanding-input replay, shared vehicle dynamics, stale/backward input,
body changes, authority pause and adversarial resolver output. Identity, finite
state, speed, 3D per-step displacement and a cumulative authority-to-preview
bound are enforced. Native terrain admission rejects missing coverage and source
obstacles. Only display state is predicted; commands, money, inventories,
collision outcomes and saves remain authoritative.

Five operator tests check transactional suspension/capability rotation, exact
rollback under an actual SQLite write lock, bounded secret-free audit, validated
advertised endpoints and resume-only admission before allocation/hydration.
These are trusted host APIs, not public account enrollment or person-level bans.

Thirty-two persistence and 26 domain regional cases cover full untrusted decode,
immutable environmental certification, exact retained page bytes, dependency
closure, duplicate IDs, source/catalog identity, changed ownership/ecology,
forged references, dormant capacity, external writers and failure-atomic commit.
Certified groups omit only unchanged dependency-closed environmental work;
current global ledgers and actual dependent cold bodies still validate together.
The host and Bevy integration suites exercise admission/hydration barriers,
dormant ownership, checkpoint/reopen and exact committed state.

The actual [server admission smoke](server-admission-v0.8.json) verifies the CLI's
`--resume-only` help/policy message, a real ephemeral listener, three host ticks,
clean final checkpoint and SQLite integrity. It is deliberately narrower than
the separate nonempty TLS and ownership fixtures.

### Input, UI and whole-game integration

The 74 native cases include character/world forms, queued edits, control
preferences, controller menu/text input, configurable camera values, wheel
profiles/calibration, real UI commands, screen transitions and process handoff.
Virtual keyboard tests apply real widget edits and reject stale/disabled input.
Wheel tests cover asymmetric/inverted endpoint calibration, separate/combined
pedals, invalid axes, focus/disconnect neutral rearming and bounded persistence.
No physical controller or wheel was available for certification.

A native layout regression checks all 16 inspector/Fleet tab cells through
100–150% scaling, stable entity/action identities, retained scroll position,
DPI-aware scrollbar placement and fixed panel bounds. A real founded-territory
case proves quick gathering skips protected unplotted resources and takes the
accessible nearby stock. The actual F5 test now uses an isolated private save
fixture and leaves no generated save in the source tree.

Existing suites continue to cover survival, harvesting, crafting, crops,
building/destruction, housing, faction roles, territory growth, finite shops,
production, real courier movement, source history and politics. The procedural
settlement integration advances 180 physical seconds through the same streamed
terrain, collision admission and Bevy schedules used in play; it requires actual
production and completed deliveries. Tests do not establish long-duration game
balance or a realistic scientific ecology/economy model.

## Finite measurements

### Regional storage

The [0.8 storage probe](regional-probe-v0.8.json) uses a 239-body/6,400-resource
fixture. A same-revision comparison alternates execution order and checks
canonical equality outside the timing scope.

| Measurement | p50 | p95 |
|---|---:|---:|
| Full merged-scope domain merge | 10.777 ms | 17.261 ms |
| Certified environmental-group merge | 3.462 ms | 8.902 ms |
| Snapshot submission, including clone | 0.408 ms | 1.265 ms |
| Complete checkpoint worker | 10.424 ms | 17.855 ms |
| Direct maintenance checkpoint | 8.433 ms | 13.644 ms |

The merge certifies 6,000 unchanged environmental records in 15 groups and
validates 656 merged-scope records. The final checkpoint reuses all 19 equal
pages, totaling 1,683,609 bytes. Full restore still validates the complete world;
it took 36.372 ms. The roughly 3.1× median comparison applies to the paired merge
operation, not the whole game or every checkpoint.

### Actual TLS/TCP sessions

Both eight-client [TLS](regional-session-load-tls-v0.8.json) and
[TCP](regional-session-load-tcp-v0.8.json) baselines pass. The extended
[16-client TLS run](regional-session-stress-tls-v0.8.json) has 140 complete living
bodies, 80 owned bodies and 1,030 resources. Eight churn/restart cycles advance
4,800 fixed steps, representing **240 physical simulation seconds**.

It performs 64 exact offline-owner comparisons, 64 authenticated resume
comparisons, 128 owner comparisons after restart and one intentional SQLite
competing-writer failure followed by exact recovery. Cargo, crafted inventory,
private ownership and accepted command sequences remain exact. The final durable
generation is 162.

| Extended TLS observation | Value |
|---|---:|
| Host updates, p95 / p99 | 19.664 / 24.758 ms |
| Advancing updates, p95 / p99 | 20.150 / 25.421 ms |
| Maximum host update | 112.079 ms |
| Updates reporting pending storage | 1,007 |
| Aggregate client receive | 291,225,409 bytes |
| Aggregate client send | 1,098,464 bytes |
| Process RSS, initial / peak / final | 16,440 / 31,668 / 31,560 KiB |

Measurements ran sequentially with the renderer stopped and Rust compilation
idle. [Measurement provenance](regional-measurement-provenance-v0.8.json)
preserves their exact executable and 134 compiled-input hashes. The subsequent
construction-only change and native UI/test fixes are covered by the final
correctness gates; they are not silently represented as the earlier measured
binaries. The four simulated minutes and RSS trace do not prove a long-term
memory plateau or concurrent-player capacity. The 112 ms maximum precludes a
guaranteed 20 Hz claim. TCP and TLS batching differs, so timings/bytes cannot be
attributed solely to encryption overhead.

## Native review, additional polish and confirmation

The review created **Mira Vale** and **Greenhaven Roads** through the actual
character, world-settings and review screens. It exercised the on-screen text
keyboard's apply/discard behavior, 150% controls at 1100×700, wheel setup with no
attached hardware and the same saved world at 1440×900. Virtual-device tests
cover controller/wheel events; native mouse/keyboard inspection does not certify
physical hardware.

The connected player path bought market goods, gathered materials, claimed a
housing plot, founded a neighborhood, crafted and placed a house and workbench,
planted wheat and set a home. It registered a transport company, funded it,
hired an employee, opened a private depot, stored goods, rented a car and renewed
the prepaid term. Save-and-return followed by a final-binary restart preserved
the entire global ledger, all regional records and metadata exactly before
resuming, including the paused 600-second rental term.

The additional visual pass found a housing-plot ribbon buried beneath sloping
terrain. Its corners now follow loaded height samples; missing pages break the
boundary instead of bridging unavailable ground. A regression covers complete
slopes, replacement pages and geographic wrapping. The final native capture
shows the complete ring. Enlarged inspector/Fleet tabs also use readable two-column
layouts, a wrapped title/wallet and a scrollbar aligned with the real viewport.
The rental chooser sits beside rental actions.

In final confirmation, returning the empty rental at its original garage paid
the quoted **60.00 credits**: **45.00 bond escrow + 15.00 for ten litres of fuel**.
The garage received those ten litres. Depot withdrawal changed carried wood
**3 → 4** and stored wood **4 → 3**, without a money change. Final personal money
reconciles as **250.00 − 61.80 rental − 1.80 renewal − 20.00 registration − 50.00
company deposit − 3.30 market purchases − 50.00 neighborhood funding + 60.00
refund = 123.10 credits**. The company retains **50.00 − 25.00 depot − 10.00
recruitment = 15.00 credits**, and the neighborhood treasury holds **50.00**.

The final build accepted owned-pickup entry, physical driving, chase/cockpit
switching, persistent parking brake, return to RTS and clock pause. Its odometer
advanced **546.367 m**. Save and quit preserved the exact final global, regional
and metadata state, committed generation **181**, and left no game processes.
The unsaved 100% session text change did not replace the explicitly saved 150%
preference. Loaded fleet recovery, household spending and multiplayer behavior
are established by the automated suites and separate network fixtures, rather
than inferred from the local review images.

The [native evidence](native-v8-verification.json) records action provenance,
retained screenshots, executable hashes and exact checkpoint comparisons. The
review uses Linux/X11 at 1440×900 and 1100×700 with Mesa software Vulkan. Images
show the actual application; no generated imagery serves as implementation
proof. This establishes interaction/layout in that environment, not a gaming-GPU
frame-rate result, Windows/macOS support or physical-device certification.

## Reproduction and scope

`scripts/verify.sh` records the standard verification sequence. Read
[GAME_FLOW.md](GAME_FLOW.md), [GAMEPLAY.md](GAMEPLAY.md),
[COMPANIES.md](COMPANIES.md), [TRANSPORT.md](TRANSPORT.md),
[CONTACTS_AND_ROUTING.md](CONTACTS_AND_ROUTING.md), [CONTROLS.md](CONTROLS.md),
[CLIENT_PREDICTION.md](CLIENT_PREDICTION.md), [OPERATIONS.md](OPERATIONS.md) and
[INTEGRATION.md](INTEGRATION.md) for the complete command and integration contracts.

The reviewed heightmap and settlement are authored synthetic content at real
coordinates. Offline fixtures do not certify global data availability, real
historical coverage, ecological truth or live-provider reliability. The
[product review](PRODUCT_REVIEW.md) records closed findings and remaining
acceptance gates. Production physics/assets, public account services,
distributed authority, long-duration balance and complete Motor Town parity
remain explicitly separate workstreams.
