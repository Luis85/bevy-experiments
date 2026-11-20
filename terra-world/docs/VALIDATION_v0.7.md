# Terra World 0.7 validation record

Validation date: **2026-10-27**. Source release **0.7.0**, wire protocol **5**,
TLS ALPN **terra-world/5**, domain save version **1**, regional database schema
**1**. The release contains **461 distinct passing Rust tests**, with zero
final failures and zero ignored tests. This is correctness evidence for a bounded
framework and reference game.

The [machine-readable release report](release-verification-v0.7.json) links the
automated, native, operational and storage evidence. Earlier results remain in
[v0.6 validation](VALIDATION_v0.6.md), [v0.5 validation](VALIDATION_v0.5.md) and
[v0.4 validation](VALIDATION_v0.4.md); their images and timings are historical.

## Automated gates

| Crate | Distinct passing tests |
|---|---:|
| `terra-cli` | 4 |
| `terra-core` | 26 |
| `terra-history` | 13 |
| `terra-net` | 86 |
| `terra-persistence` | 29 |
| `terra-sandbox` | 161 |
| `terra-stream` | 20 |
| `terra-bevy` | 65 |
| `terra-demo` | 57 |
| **Total** | **461** |

The Bevy total is 44 render-library cases plus 21 integration cases. Its 25
headless-library cases were also run and overlap the render-library set; they
are counted once. The non-Bevy gate has six empty documentation-test harnesses,
and Bevy's documentation harness is empty. The examples, CLI smoke, native
interactions and earlier targeted runs contribute no extra Rust test cases.

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

The recorded runs use Rust 1.99.1, pinned Bevy 0.20.0, Linux x86_64, cached
locked dependencies and two Cargo jobs. Local TCP/TLS tests required permission
to open localhost sockets in the test environment. Successful authorized reruns
are recorded; executor-denied socket attempts are not counted as passed tests.

### Companies, contacts and physical logistics

The 15 company tests cover registration, ownership and name bounds, transfers,
finite wages, passenger queues, freight inventory/reservations, aggregate storage
capacity, fare escrow, physical time and odometer proof, pause/release, stalled
routes, source reload and dormant loaded commitments. Roadside regressions cover
loaded passenger and freight vehicles recovering from zero fuel/low condition,
later delivery, and atomic rejection of invalid price, funding, reach, movement,
assignment, tank and station-stock cases.

Continuous contact tests cover geodetic translation, conservative rotation,
vertical separation, date-line/polar coordinates, finite values, existing-overlap
escape and spatial relocation. Domain movement tests exercise occupied goals,
clear reachable goals, pedestrian routing, traffic lanes, safe forward clearance,
terrain rejection and physical courier deliveries. Paired service-bay reservation
and total destination stock admission prevent separate deadlock/overflow cases.
These remain kinematic admission tests, not rigid-body physics certification.

Actual TCP driving and replication suites exercise authoritative movement,
contact with occupied bodies, input timeout, replay rejection, restored heading,
private company deltas, control transfer and durable resume. A real host does
not accept a client-supplied coordinate as proof of driving or paid arrival.

### Regional lifecycle, storage, privacy and operations

Twenty-nine persistence tests cover pinned identity/catalog/source rules,
checksums, bounded payloads, complete validation, immutable cold certificates,
external SQLite changes, one-worker lifecycle, failure-atomic installation,
backup, export and destination guards. A deterministic competing-writer case
changes the database after preparation; the write barrier rejects the stale
checkpoint without overwriting that change.

Dormant owners retain accounts, inventory and obligations while their bodies and
homes can retire. Personal jobs and commercial trip clocks keep their remaining
physical duration; no offline wages, movement, need decay or catch-up earnings
are granted. Loaded passenger/freight jobs and a previously paused employee are
covered separately. A legacy owned courier is tested at pickup, loaded delivery
and loaded return across 2,000 dormant physical seconds, checkpoint, reopening
and resume. Forging a matching private cargo reference and receipt still fails
complete merged-world validation.

Fourteen actual host regional cases, a cold-owner restart/resume case, and nine
Bevy regional integration cases exercise hydration, authority barriers and
recoverable persistence. Five TLS integration cases cover verified command and
resume, CA/hostname failures, plaintext rejection, bounded configuration and
obsolete ALPN. Rejected or duplicate capabilities cannot cause an owner to hydrate
or activate before admission. This does not provide public account enrollment.

[Actual-process operations evidence](server-operations-v0.7.json) records exact
server JSON → SQLite → JSON, `terra-worldctl inspect`/`verify`, consistent backup,
portable archive roundtrip, host validation after reopening, overwrite refusal,
database-alias rejection, missing inspection without file creation, private
output permissions, remote plaintext guards and a configured localhost TLS
listener. The fixture has zero principals; nonempty company/owner resume and TLS
handshakes are established by the separate integration suites. The final narrow
navigation correction did not change these storage/CLI paths.

### Native input and presentation regressions

The 57 demo cases include names and queued text edits, focus and capture,
reserved F5/F9 suppression, contextual Space, RTS focus-loss cancellation,
saved preference validation and bounded unsaved-control handoff, controller
sampling/deadzone, screen transitions, world/profile paths and source presets.
A regression runs a real `App::run` and verifies the external return handoff.
Fleet UI cases cover actual assignments, quotes and prerequisite feedback.
Wheel/model rebuilding and route endpoint/map clipping are checked separately.
No physical controller or steering wheel was available for certification.

## Finite regional measurements

The [0.7 regional report](regional-probe-v0.7.json) contains two synthetic
fixtures. The population fixture has **239 complete / 59 active bodies** and
**6,400 complete / 400 active resources**. It runs 500 steps: 25 physical seconds
and 250 calendar seconds. It has no sockets, renderer, imported collision or
live provider. The same report records 33 verified durable generations.

| Measurement | Result |
|---|---:|
| Direct checkpoint p50 / p95 | 30.905 / 38.713 ms |
| Submission including snapshot clone p50 / p95 | 0.483 / 5.369 ms |
| Worker validation and commit p50 / p95 | 32.104 / 46.387 ms |
| Maximum nonblocking poll, 473 calls | 0.004898 ms |
| Explicit complete restore | 32.695 ms |
| Guarded cache hits / repeated cold decodes | 98 / 0 |

Every complete/worker checkpoint, roundtrip, backup and restart matched exactly.
The second fixture has 16 synthetic owners, 15 dormant. Resident owned bodies
fall from **80 to 5**, and resident referenced homes from **16 to 1**. Accounts,
inventory, offline bodies, needs, orders and resources remain exact; restart and
reactivation validate. The retirement checkpoint measured **4.008 ms**.

These are shared-environment measurements from an optimized development build
with debug assertions and SQLite 3.53.2. They do not establish concurrent-player
capacity or memory savings in bytes. Authority still pauses while one worker
reconstructs and validates the bounded complete world. Snapshot cloning and
installation remain main-thread work. Startup, explicit complete restore/export
and final drain remain synchronous. See [PERFORMANCE.md](PERFORMANCE.md) for the
workload distinctions and historical comparisons.

## Native review and confirmation

The [native evidence record](native-v7-verification.json) lists actual actions,
state checks and hashes of the retained screenshots. Native review used Linux/X11
at 1440×900 and 1100×700 with Mesa software Vulkan. Captures show the game itself;
no generated images are used as implementation evidence.

An initial batched review exposed control-handoff, layout, input, wheel/guidance
and vehicle-clearance problems. The combined improvement pass corrected those
paths. During confirmation, the real heightmap exposed a further perpendicular
turn blocked by nearby traffic. Its exact saved pose and actual terrain admission
were inspected, the narrow contact-clearance rule was corrected, and the saved
loaded trip was resumed through the native UI. The record distinguishes this
functional blocker correction from additional optional visual review.

The completed flow includes spaced character/world names, an unsaved I binding
following menu → world → menu before explicit Save, company registration,
deposit/hiring, a custom two-stop passenger route, and a quoted 10-litre roadside
purchase. The same saved loaded trip resumed after the correction, completed at
Hillside, and returned to Town. A crossing pedestrian caused a later deliberate
stop; the normal safe-pause action completed the committed delivery and parked.
The empty vehicle was released for personal driving, cockpit/chase switching,
parking, save-and-return, and another Continue.

The final company ledger is **50.00 − 10.00 recruitment − 30.00 roadside service
+ 17.28 fares − 1.46 wages = 25.82 credits**, with two paid deliveries. Personal
cash remains **180.00 credits**. Native checkpoint comparison verifies exact
company/account/profile/inventory restoration, and the parked vehicle retains
its position, fuel, condition and **563.4715093030702 m** odometer. The earlier
separate world remains unchanged, and Save and quit leaves no game processes.

The native report distinguishes UI observations from transaction and network
invariants established by tests. Gamepad input is exercised through virtual
inputs; physical hardware, Windows/macOS, gaming-GPU frame rate, comfort and
accessibility certification remain open. The geometric environment and vehicles
are prototype art, despite improved terrain, lighting, controls and readability.

## Reproduction and scope

`scripts/verify.sh` contains the standard sequence. See [GAME_FLOW.md](GAME_FLOW.md),
[COMPANIES.md](COMPANIES.md), [CONTACTS_AND_ROUTING.md](CONTACTS_AND_ROUTING.md),
[CONTROLS.md](CONTROLS.md), [OPERATIONS.md](OPERATIONS.md) and
[INTEGRATION.md](INTEGRATION.md) for implementation contracts and entry points.
The reviewed heightmap and settlement are explicitly authored synthetic content
at real coordinates. Offline fixtures do not certify live-provider availability,
global open-data completeness or historical coverage.

The [product review](PRODUCT_REVIEW.md) records the closed findings and next
acceptance gates. [Motor Town coverage](MOTORTOWN_PARITY.md) and the
[MMORPG foundation](MMORPG_FOUNDATION.md) remain explicit about partial and
missing capabilities. This release does not claim complete Motor Town parity,
AAA assets/physics, production accounts, client prediction, distributed authority
or demonstrated MMORPG concurrency.
