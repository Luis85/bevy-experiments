# Terra World 0.10 validation record

Validation date: **2026-10-28**. Source release **0.10.0**, wire protocol **7**,
TLS ALPN **terra-world/7**, domain save version **1**, regional database schema
**1**. The final automated gates contain **655 distinct passing Rust
tests**, with zero failures and zero ignored tests. These are correctness results
for the bounded SDK and reference game.

The [release report](release-verification-v0.10.json) connects source hashes,
commands, executable identity, recovery evidence and native confirmation. The
[0.9 validation record](VALIDATION_v0.9.md) preserves previous measurements;
earlier captures and process probes are not relabeled as current-release proof.

## Automated gates and counting

| Crate | Distinct passing tests |
|---|---:|
| `terra-bevy` | 90 |
| `terra-cli` | 4 |
| `terra-core` | 33 |
| `terra-demo` | 91 |
| `terra-history` | 13 |
| `terra-net` | 115 |
| `terra-persistence` | 35 |
| `terra-sandbox` | 242 |
| `terra-stream` | 32 |
| **Total** | **655** |

The [gate report](verification/v0.10/gates.json) records every command, duration,
exit code and log. [Distinct test identities](verification/v0.10/distinct-tests.json)
deduplicate a case repeated across Bevy feature configurations. Empty harnesses,
examples, native interactions and the storage probe add no Rust test cases.
Durations are wall-clock observations on a shared development machine, not game
performance or capacity measurements.

The complete release sequence passes with pinned Rust **1.99.1**, Bevy
**0.20.0** and locked cached dependencies on Linux x86_64. The standalone
procedural gate used four Cargo jobs; the consolidated commands used three:

```sh
cargo fmt --all -- --check
cargo test --workspace --exclude terra-demo --exclude terra-bevy --locked --offline -j3
cargo test -p terra-sandbox --test company_depots six_session_warehouse_career_conserves_finite_goods_cash_and_recovers_scarcity --locked --offline -j3 -- --exact --nocapture
cargo test -p terra-bevy --no-default-features --locked --offline -j4 --test procedural_settlement -- --nocapture
cargo test -p terra-bevy --no-default-features --locked --offline -j3 -- --skip default_munich_procedural_settlement_produces_and_physically_delivers
cargo test -p terra-bevy --lib --locked --offline -j3
cargo test -p terra-demo --locked --offline -j3
cargo check -p terra-demo --locked --offline -j3
cargo run -p terra-bevy --no-default-features --example headless --locked --offline -j3
cargo run -p terra-bevy --no-default-features --example custom_provider --locked --offline -j3
cargo run -p terra-bevy --no-default-features --bin terra-assets --locked --offline -j3 -- validate crates/terra-bevy/assets/wildlife-pack/pack.json
cargo run -p terra-persistence --example recovery_probe --locked --offline -j3 -- --output docs/recovery-verification-v0.10.json
cargo build -p terra-demo --locked --offline -j3
```

Actual TCP/TLS tests use localhost sockets, with the runner permitted to bind and
connect. They are not mocked transport calls. The native build needs the Linux
development libraries described in the [README](../README.md). `--offline`
requires dependencies to have been fetched previously; it is a reproducibility
condition for this recorded run, not a first-install instruction.

The separately completed procedural test is recorded once. Before the remaining
Bevy tests ran, the consolidated runner checked that all 296 source/config/data/
asset hashes exactly matched its standalone manifest and verified its passing
binary and log hashes. The `--skip` above avoids running that identical case a
second time; it is present as its own passing gate and counted in the total.
Its recorded 197.96 seconds covers test execution, excluding Cargo build time.

## Source freeze and executable provenance

The [source manifest](verification/v0.10/source-hashes.json) records SHA-256
digests for **296** Rust, Cargo/toolchain, config, prepared-data and
asset inputs. All remained unchanged throughout the final gate sequence and
the final native confirmation. Documentation and evidence are produced after
the compiled-source freeze and do not alter those inputs.
A separate [fixture manifest](verification/v0.10/embedded-fixture-hashes.json)
records seven embedded JSON/TLS fixture inputs, checked unchanged against the
imported baseline and again after the final native confirmation.

The [standalone procedural record](verification/v0.10/procedural-standalone.json)
retains the build command, passing test command, binary hash, source manifest
and continuity evidence. A full manifest was not captured immediately before
that build. The record instead identifies the two known changes from the prior
complete snapshot, both completed before linking, input modification times,
execution without recompilation, and the matching manifest checked afterward.
It does not present a post-build manifest as an earlier snapshot.

The final native executable SHA-256 is:

```text
b4add5c97c124f8edfc278c5bd63ce7b9632a22e0ee3bfa2119c65b3b6407405
```

Development review used an earlier binary for character/world creation and the
first supply delivery, followed by a separately identified 0.10 development
binary for restore, community founding, compact navigation and driving. The
[development-native record](verification/v0.10/native-review.json) retains that
scope and its executable identity. [Final native evidence](native-v10-verification.json)
identifies the later release executable, checks and captures. Neither record
claims an earlier screenshot came from the final binary.

## What the correctness gates establish

### Sustained guidance and existing authority

Domain tests exercise read-only goal derivation, funded acceptance, exact goods
consumption and exactly-once payment, ownership/territory access, real crafting
costs, cyclic catalogs, affordable finished goods, stale displayed intent,
accepted transport assignment identity and save/resume. Resource search is
bounded to the nearest 128 candidates, with indexed permission checks. The UI
rederives an action before submitting the ordinary authoritative command.

Suggested journey prioritizes water, food and energy at 20% or below. It uses
the registered driver's actual avatar, directs a driver to park and exit, and
offers real consumption/acquisition or the existing rest controls on foot.
Explicit goal tracks remain selected. Regression checks ensure no automatic
vehicle exit, consumption, rest command or cancellation occurs simply by opening
guidance. Native integration also checks the selected person and command panel.

Real-engine guidance tests accept, travel through the living-world schedule and
deliver on the connected prepared landscape. Stopping-point admission checks
the final heading, wheel support and body envelope; a dry center cannot conceal
unsupported corners. Layout regressions cover the compact activity chooser,
150% text, primary action visibility and the actual wrapped banner's hit area.
See [GUIDANCE.md](GUIDANCE.md).

### Animated asset admission and runtime behavior

The headless pack CLI validates the included five-model wildlife example. Tests
exercise immutable content identity, bounded GLB geometry/material/skin/animation
data, hierarchy and reachable-mesh requirements, transform envelopes and safe
regular-file inputs. All clips count toward admission even if unused by a binding;
shared samplers/accessors cannot multiply allocation or validation work beyond
the budgets. Per-binding animation choices and speed policies remain independent.

Runtime tests cover readiness-gated procedural fallback, missing/failed assets,
rejected hierarchies, actual-speed gait selection, inherited visibility,
first-person suppression, final STEP keys, and pause/resume of both clip time
and crossfade weight. Calendar speed is independent from physical simulation
pause. The generator reproduced the five GLBs and manifest byte for byte; the
[reproducibility record](verification/v0.10/wildlife-reproducibility.json) retains
their hashes. Native diagnostics establish that the example actually loads.

The pack binds wildlife, preserves character appearance and changes no saved
authority, collision geometry or economic state. Format and workload limits
remain explicit in [ASSET_PACKS.md](ASSET_PACKS.md).

### Autonomous handling, traffic and physical continuity

Autonomous vehicles supply bounded controls to the same `drive_actor` handling
used for controlled driving. Tests cover turning and braking, four-wheel support,
grip/load, finite fuel, denied terrain, body sweeps and accepted curved-distance
accounting. Persisted ambient handling survives complete saves and cold regional
restoration. Vehicle commands cannot set a waypoint pose or claim unsupported
travel as a delivery.

Cross-review and the unchanged procedural settlement regression exposed recovery
cases beyond a flat route: forward following gaps must allow a safe reverse away
from a leader, and moving pedestrians must retain a chosen walking detour rather
than repeatedly undoing their sidestep. Bounded clearance and detour routes keep
their original work destinations. Every step still requires loaded source
support, intersection admission and full-body contacts. The final original
**180-second simulated procedural settlement gate passes**, with actual physical
courier deliveries and ordinary production, traffic and facility collision.

Two queue regressions retain their original 0.1-second duration and compare
active versus parked neighbors in the same physical arrangement. They require
positive bounded acceleration when the queue is clear and yielding when an
active peer blocks it; the former demand for more than half a metre from rest
in one tenth of a second no longer substitutes for queue correctness.

The finite-goods and populated-settlement domain gates also retain their
original **300-second** and **120-second** simulated bounds. Their review found
that Patrol vehicles could undo safe reverse clearance and that checking only
a detour's corners admitted unusable corridors. Temporary Patrol routes now
remain consistent across controller, traffic intent and save/resume; complete
two-leg admission includes source envelopes, traffic, body sweeps and the
controller's reverse-facing rule. Stationary obstacles retain complete detour
priority. If neither complete detour admits, an already-admitted outward
clearance can persist, preventing the original route from immediately undoing
the escape. Captured stationary-avatar cases include deterministic save/resume.
Exact goods conservation and physical delivery assertions remain.

The flat living-world integration waits for the physical delivery condition
within a bounded window; its former fixed cutoff was just before the first
delivery under bounded turning. No reward, conservation, restore or arrival
assertion was removed. Actual TCP RTS tests likewise wait for physical distance
and the Idle order, rather than assuming a pose can be reached immediately.

New multiplayer crews need supported and unoccupied placement for every member,
including vehicles, against resident and dormant bodies. Failure remains atomic.
Resumed principals retain their saved positions. See [VEHICLE_DYNAMICS.md](VEHICLE_DYNAMICS.md)
and [CONTACTS_AND_ROUTING.md](CONTACTS_AND_ROUTING.md).

### Finite warehouse career

The deterministic six-session warehouse scenario uses the ordinary domain
movement and economy on a flat corridor. It covers loaded return, cold restore,
payroll scarcity, paid finite-stock fuel recovery, cancellation/retry and final
source exhaustion. Its final measured results are:

| Fixture observation | Result |
|---|---|
| Sessions / physical deliveries | 6 / 24: 12 private transfers and 12 shop sales |
| Save/resume checkpoints | 7, including loaded cold-region restore and payroll scarcity |
| Physical world time | 2,967.45 seconds, including dormant and off-duty intervals |
| Goods sold / customer purchases | 96 water / 96 water |
| Sales / wages / paid roadside service | 28.80 / 15.49 / 18.00 credits |
| Operating cash change excluding owner transfers | **-4.69 credits** |
| Conserved money and material, including consumption | 13,900.00 credits; 224 water; 40 bread |
| Exhausted-source result | WaitingGoods; no income or payroll on repeated idle ticks |

The operating figure excludes setup and owner transfers. It is a balance
finding, not a profitable-career claim. Starting harvest, short routes, authored
prices and a finite customer budget do not establish long-career pacing. Exact
scenario interpretation and reproduction are in [COMPANIES.md](COMPANIES.md).

### Connected terrain, coverage and provenance

Prepared collections validate 1–32 canonical child bundles, reject overlapping
interiors, duplicate/nested entries and disconnected or corner-only joins, and
check shared edges at every knot of both rasters. Mixed resolutions and isolated
seam spikes are covered. Exact rectangle-union admission rejects concave holes
and narrow gaps instead of treating the collection's bounding box as ground.

Content identity includes child bytes and seam policy and is independent of
list order. Long external feature IDs normalize into bounded gameplay IDs while
retaining original provenance. Compatible short IDs and legacy single-bundle
semantics remain unchanged. The original two-bundle example covers 81 level-15
tiles; this is authored synthetic geography with bounded regional coverage.
Native and server configuration use a genuinely covered focus point and retain
source compatibility checks. See [SOURCE_BUNDLES.md](SOURCE_BUNDLES.md).

### Operational recovery and larger stored state

Persistence tests cover read-only maintenance, exact snapshots, copied-generation
receipts, retries, source/destination aliasing, existing outputs and sidecars,
invalid payloads and exclusive atomic publication. `terra-worldctl recover`
creates a new verified database and does not overwrite an incident source.
Application-specific private metadata remains subject to the host's validator.

The [recovery probe](recovery-verification-v0.10.json) creates **1,027 nonempty
regions**, **6,144 resources** and **47 living bodies**, with **1,023 dormant
regions**. It verifies exact full state, private metadata, generation and dormant
records after backup and recovery. Corrupt input leaves no output, and backup
source bytes remain unchanged. Final timings and payload size are recorded in
that report; they are machine-specific observations.

This is a storage workload with zero clients, no renderer and no imported
terrain. It does not certify populated-world throughput. Region/payload caps,
single authority and full-validation barriers remain. The copy retry deadline
does not preempt filesystem operations or full validation. See [OPERATIONS.md](OPERATIONS.md)
and [SCALING.md](SCALING.md).

### Existing foundations retained

The full suites retain construction, housing, farming, survival, wildlife,
resources, ownership and factions; finite production and shops; transport,
rentals and company commitments; history/politics and temporal/source identity;
regional transactions, dormant ownership and private host metadata; actual
TCP/TLS replication, backpressure, reconnect and sequence replay rejection;
and native launcher, input, preferences and save flows. Equal protocol numbers
do not certify arbitrary mixed executable versions.

## Native experience and confirmation

The final **0.10.0 executable**, identified above, resumed the fictional
**Riverbend Commons** connected-heightmap world with **Mira Vale**, the ochre
appearance and persisted **150% text**. The 1100×700 window retained the
**205.68-credit** balance, **50 XP**, completed supply contract and **Willow Brook**
neighborhood with its **50.00-credit treasury**.

The final native sequence exercised the complete urgent-water loop. The
registered pickup's driver had **0% water**. Suggested guidance displayed
**Park and exit to drink**; its action opened the correct vehicle's Driving
panel. Parking and the normal Exit on foot action changed the goal to acquiring
water. **Go to water** issued ordinary RTS movement, moved the avatar clear of
the parked pickup and reached a real accessible resource. **Gather 1 water**
then changed to **Drink 1 water**. Consumption raised the displayed need to
**60%** and restored **Claim a housing plot** as the continuing goal. The final
archive contains about 59.90% water after ordinary subsequent decay, no extra
water in inventory, and the unchanged balance, progression and territory.

The final pack diagnostics reported **five models and 64 ready instances**,
with **zero loading, fallback, rejected or failed instances**. The native world
had 49 rendered chunks and 107 living bodies. Session telemetry recorded no
world or simulation error. These are active-view observations, not scale or
performance certification.

Keyboard navigation scrolled **Save and quit** into view and activated it.
The process exited **zero**, with a completed **Graceful exit checkpoint 137**,
no pending persistence work and no storage error. A read-only export verified
the saved state; the private world/database are not included in the release.
The final native record retains the exact executable, command, telemetry and
capture hashes, and all 296 compiled/config/data/asset inputs remained unchanged.

Earlier development review separately covers character/world creation,
accept → physical travel → delivery of four wood, community founding, manual
driving up to an observed 26 km/h, first-person visibility and compact navigation.
Its captures remain labeled as development evidence; the final confirmation
above does not relabel them as captures of the release executable.


Native review ran on Linux/X11 at **1440×900** and **1100×700**, including **150%**
interface text, using Mesa llvmpipe software Vulkan. It establishes real rendering,
interaction and persistence in that environment. It does not measure gaming-GPU
frame time or certify Windows/macOS, screen readers, localization, physical
controllers, wheels or force feedback.

## Product boundaries and next acceptance work

A [saved dense-queue diagnostic](verification/v0.10/traffic-exit-limit.json)
resolves its initial rearward steering trap and advances two waypoints, then
holds at a different junction whose outgoing exit is occupied under the
configured clearance bounds. Its oldest request, all 12 wood and exact per-item
conservation remain. No cargo return or delivery completed during that diagnostic.
This is an explicit local-recovery limit, separate from the passing fresh-world
acceptance gates; no admission rule was relaxed to manufacture progress.

The [product review](PRODUCT_REVIEW.md) explains the improvements and remaining
acceptance work across player experience, economy, art production, accessibility,
source trust, operations and SDK integration. Local pedestrian/vehicle detours
remain bounded route suggestions; an obstructed final service point remains
unavailable. Full tire/suspension and trailer dynamics, unrestricted navigation,
broader authored art/audio, long-career balance, geographic distribution and
meaningful populated-world/network soaks require further implementation and
evidence. This release is a verified reference-game and SDK increment, not a
production MMORPG or AAA quality certification.
