# Terra World 0.9 validation record

Validation date: **2026-10-27**. Source release **0.9.0**, wire protocol **7**,
TLS ALPN **terra-world/7**, domain save version **1**, regional database schema
**1**. The final automated gates contain **582 distinct passing Rust tests**,
with zero final failures and zero ignored tests. These are correctness results
for the bounded SDK and reference game.

The [release report](release-verification-v0.9.json) connects automated,
source-provenance, executable and native evidence. The
[0.8 validation record](VALIDATION_v0.8.md) remains preserved separately;
its test counts, captures and performance measurements are historical.
Native capture conditions and the final confirmation status appear below.

## Automated gates and counting

| Crate | Distinct passing tests |
|---|---:|
| `terra-cli` | 4 |
| `terra-core` | 33 |
| `terra-history` | 13 |
| `terra-net` | 111 |
| `terra-persistence` | 32 |
| `terra-sandbox` | 206 |
| `terra-stream` | 25 |
| `terra-bevy` | 72 |
| `terra-demo` | 86 |
| **Total** | **582** |

The [headless report](headless-verification-v0.9.json) records **424** cases
across the seven non-Bevy crates. Bevy contributes **51** render-library cases
and **21** integration cases. Its **31** no-default-feature library cases also
pass, overlap the render-library set and count once. The final native suite
contributes **86**. The six non-Bevy documentation harnesses contain no examples;
empty harnesses, targeted reruns, examples, process checks and native interactions
add no Rust cases.

All recorded Cargo gates use locked cached dependencies, offline mode and two
jobs. The logs set `CARGO_BUILD_JOBS=2`; these equivalent commands expose that
limit as `-j2`:

```sh
cargo test --workspace --exclude terra-demo --exclude terra-bevy --offline --locked -j2
cargo test -p terra-bevy --no-default-features --offline --locked -j2
cargo test -p terra-bevy --lib --offline --locked -j2
cargo test -p terra-demo --offline --locked -j2
cargo check -p terra-demo --offline --locked -j2
cargo build -p terra-demo --offline --locked -j2
cargo run -p terra-bevy --no-default-features --example headless --offline --locked -j2
cargo run -p terra-bevy --no-default-features --example custom_provider --offline --locked -j2
cargo fmt --all -- --check
```

The native typecheck/build, formatting check and both examples pass. The recorded
toolchain is Rust **1.99.1**, with pinned Bevy **0.20.0**, on Linux x86_64.
Actual TCP/TLS fixtures require localhost sockets. Test durations were collected
during development/native review and are not performance measurements.

## Source freeze and executable provenance

The initial gate freeze retains **193** Rust source/test/example and Cargo
manifest/lockfile hashes. A subsequent native review found a footer layout
defect and a nested-container keyboard scrolling defect. Only
`crates/terra-demo/src/interface.rs`, `crates/terra-demo/src/driving.rs` and
`crates/terra-demo/src/command_input.rs` changed after that snapshot. Initial and
final hashes for all three are retained; the applicable non-Bevy and Bevy gate inputs stayed
unchanged. The native suite, native typecheck, executable build and formatting
check were rerun after the isolated correction. The reported total uses the
final **86-test** native suite.

The final native executable SHA-256 is:

```text
00d46588a66d07a7c7c5040de1f177058fd389e833d01a552536bc7be9e2fd3a
```

Earlier native captures use distinct initial `8aa3…`, world-creation `941…`
and footer/vehicle-confirmation `455e…` binaries. Their identities remain separate in the
[native evidence](native-v9-verification.json); an earlier image is not relabeled
as proof of this final executable. The process smoke has its own server/client,
fixture and source-bundle hashes. Data files and screenshots have separate
provenance from the 193 compiled inputs.

## What the correctness gates establish

### Geographic sources and hosting

Semantic heightmap tests cover shared raster edges, absent coverage, normalized
and clipped geographic geometry, raster-grounded vertices, retained tags and
independent attribution. Edited manifest/raster/GeoJSON content changes semantic
bundle identity. Duplicate IDs, unsupported geometry/holes, path escape and
invalid dimensions reject. Expanded property budgets bound multipart and clipped
attribute growth; disconnected/touching concave results reject instead of
filling an invented land surface.

Native/host admission tests require matching source identity and reject missing
spawn coverage or incompatible mixed sources. The host registers covered
neighbors before seeding and does not invent procedural replacement terrain.
Native Join validates a fixed local WorldConfig before creating session storage.
The data must already exist on both machines; gameplay networking does not
download source files. See [SOURCE_BUNDLES.md](SOURCE_BUNDLES.md).

Existing history/politics cases continue to cover explicit epochs, half-open
temporal selection, source revision, polygon holes/date-line behavior, bounded
pack paths and rejection of absent historical coverage. The bundled dated
scenarios and new heightmap land cover remain explicitly synthetic.

### Vehicle support, load and presentation

Core/domain/adapter cases exercise four-wheel support on rotated slopes and
across the date line; missing/invalid samples; roughness; source-road material
precedence and bridge support; slope brake hold/release; low-grip slipping; and
the shared longitudinal/cornering force budget. Actual cargo, onboard passengers
and attached trailer tare affect effective load without counting waiting
passengers or trailer cargo twice. Pickup/delivery, fuel, braking and save/restore
cases preserve those derived values.

Owner prediction samples support during each replayed step and stops on
unavailable terrain. It remains a bounded display preview: authority, inventory,
odometer and fuel do not change. Native wheel tests exercise independent terrain
travel, spring subdivision, true pause, missing support and model replacement
without changing gameplay state.

These cases qualify the reference surface model described in
[VEHICLE_DYNAMICS.md](VEHICLE_DYNAMICS.md). They do not establish dynamic
suspension forces, tire contact-patch physics, rigid-body crashes, rollover,
articulated trailer contacts or an equivalent autonomous-vehicle solver.
Existing envelope sweeps, occupied construction, directed road rules and
intersection regressions remain in the final suites.

### Warehouse economy and physical recovery

The new warehouse cases exercise exact quote-checked buys/sales using real shop
stock and company/shop money, stale-quote rollback, mixed-capacity rejection,
buyer affordability and partial loads. Warehouse sales reserve finite buyer
cash when loading and pay only after valid physical delivery. Internal private
transfers receive no public fare. Source refund and destination arrival space
cannot be consumed by later deposits or market purchases.

Return, hold, relief handover, dormant cold-region restore and failed refunds
preserve the exact goods/payment commitment. Capped recipients wait rather than
discard value. Existing company/rental tests retain wage settlement, finite fuel,
lease expiry, physical return and privacy behavior. These are domain transitions,
not arithmetic duplicated in a UI mock.

The deterministic six-session career in
[company_depots.rs](../crates/terra-sandbox/tests/company_depots.rs) records:

| Fixture observation | Result |
|---|---|
| Physical deliveries | 24: 12 private transfers and 12 shop sales |
| Save/resume checkpoints | 7, including loaded cold-region restore and payroll scarcity |
| Physical world time | 2,638.45 seconds, including dormant/off-duty intervals |
| Goods sold / customer purchases | 96 water / 95 water |
| Sales / wages / paid roadside service | 28.80 / 8.91 / 18.00 credits |
| Operating cash change excluding owner transfers | +1.89 credits |
| Conserved money and material, including recorded consumption | 13,900 credits; 224 water; 40 bread |
| Exhausted-source result | WaitingGoods; no income or payroll on repeated idle ticks |

The fixture includes actual fuel exhaustion, paid finite-stock recovery,
cancellation/retry and replenishing payroll through an owner deposit. Its
isolated flat corridor, finite starting harvest and short routes do not
establish full-career profitability. Registration, hiring, bays and harvest
acquisition are outside the operating cash figure. Exact interpretation and
reproduction are in [COMPANIES.md](COMPANIES.md).

### Network backpressure and trusted administration

Actual TCP and TLS condition tests use an opaque relay with **25 ms one-way
scheduled delay**, **0–15 ms ordered jitter**, writes of at most **997 bytes**
and **128 KiB buffering per direction**. Pausing a receiver fills real socket
buffers. The host defers optional views before advancing their baselines,
keeps queued frames ordered and continues serving a healthy peer. Drain restores
exact actor/private replicas without spurious deferral-induced resync.

The shared journey includes a reliable craft, disconnect/save/restart/resume and
rejection of its accepted sequence. A separate response-headroom regression
proves that a reliable craft waits without mutation, then executes once after
capacity returns. Failed resync sends cannot create phantom pending requests;
rate-limited retries remain bounded.

The **512 principal actor records** include **510 simple kinematic stress
records**, not 512 living NPCs or concurrent players. Relay write fragments are
not Internet packets, and ordered jitter does not simulate packet loss/reordering.
See [NETWORK_REPLICATION.md](NETWORK_REPLICATION.md).

Trusted operator cases provision a dormant principal while guests remain closed,
validate capacity/safe resident spawn/advertised endpoint, and preserve exact
identity after restart. A real SQLite lock rejects without consuming IDs,
capabilities or cold ownership. Regional provisioning commits the candidate
principal, capability and audit before publication. In-memory hosts must save
before private capability delivery. Suspension/rotation and recipient privacy
remain covered. These APIs do not supply public accounts or an authenticated
operator service; see [OPERATIONS.md](OPERATIONS.md).

### Native input and integrated gameplay

The final native cases cover launcher forms, source configuration, preferences,
real UI commands, screen transitions, process handoff, private text drafts,
management focus and controller RTS capture. Hidden/disabled actions cannot be
selected; focus reveals actual scroll bounds. Escape and text input do not leak
into movement or atlas drawing. Concurrent edits and a full queue preserve
draft retry. Normal typing and Enter work in the on-screen keyboard even when
ordinary gamepad input is disabled.

Camera grounding remains active when management opens before terrain loads.
Entering management selects the current panel. The final footer regression,
`interface::layout_tests::scaled_footer_reserves_the_same_world_panel_and_floating_hud_bounds`,
passes **100% → 150% → 100%** scaling: footer height is
**96px × text scale**, the viewport reserves that height **+ 26px**, and panels,
floating HUDs and world hit testing respect the shared bounds.

Existing suites retain survival, gathering, crafting, farming, housing,
construction/destruction, wildlife/resources, faction/territory rights, production
and physical courier movement. The Bevy settlement integration advances the
streamed terrain and ordinary schedules through actual production/deliveries.
Virtual device tests qualify event handling and saved wheel calibration; no
physical controller/wheel certification follows.

## Executable and native journeys

The separately scoped [server process smoke](server-admission-v0.9.json) passes
**11 checks** using actual server/client executables. These cover procedural
two-client play; JSON and regional restart/identity; conflicting restore modes
and export aliases; historical/political admission and mismatches; matching
semantic-heightmap admission with crafting; edited/missing/mixed source rejection;
and nonblocking rejection of special-file inputs. Its earlier executable hashes
remain explicit. The eleven checks are not added to the Rust test total.

The recorded native journey used the actual Join form with the matching local
heightmap source and a loopback authoritative host. Ordinary on-screen-keyboard
typing and Enter created **ValleyFreight**. Registration charged **20.00**,
personal-to-company funding moved **50.00**, and a private bay cost **25.00**.
Buying one bread for **2.90**, then selling it for **1.40**, left durable personal
money **180.00**, company money **23.50** and an empty bay. These are real
committed transactions; the spread prevents a buy/sell wealth loop.

Native review uses Linux/X11 and Mesa software Vulkan. Actual application
captures establish interaction/layout in that environment, not gaming-GPU frame
time, Windows/macOS support or physical-device certification. Individual capture
binary identities remain in [native-v9-verification.json](native-v9-verification.json).

### Final native confirmation

The native local journey created **Rowan Vale** and **Valley Commons** through
character creation, heightmap settings and the review screen. The regional world
uses the semantic v2 bundle, radius 3 and a 1× clock. The final build resumed it
with the expected profile, source identity and player state. Its visible loaded
population was **107 bodies**, including **64 wildlife**; these are reference
population counts, not concurrent-player measurements.

The final 1100×700 confirmation used the saved **150% text** preference. All
status and management instructions fit, including the extra paused-clock text.
Arrow navigation reached **Mark course start** below the initially clipped
Driving content and scrolled it into view. Escape returned to the world without
opening the session menu. The added nested-container regression previously
reproduced a zero-scroll failure; the correction requires actual vertical
scrolling on the ancestor, skips ordinary/horizontal-only nodes and produces
69 logical pixels of movement in its 2× DPI fixture. Both the native confirmation
and the final 86-case suite include this correction.

The vehicle-confirmation build accepted first/third-person actor possession,
entry into the owned pickup, physical acceleration, braking, parking and
cockpit/chase switching. Its odometer reached **49.832706 metres**, with
**48.743168 litres** of fuel, zero speed and the parking brake engaged. The
complete derived vehicle state and paused clock remained exact after the final
binary reopened that save. Personal money stayed **250.00 credits** and carried
inventory stayed **30 wood, 20 stone and 8 wheat seed**.

The first paused checkpoint and its graceful close, generations **87 → 88**,
match in complete global state, every retained regional payload and metadata.
The final build's **Save and quit** button committed generation **118**; its state
matches the final paused checkpoint **114** by the same exact comparison.
Only the database generation changes. Across the intervening camera-driven
reopen, the vehicle, clock, money, inventory and profile are compared explicitly;
regional resident/dormant layout is allowed to change with the camera. SQLite
integrity passes. The [16 native captures and aggregate checks](native-v9-verification.json)
retain per-build provenance without distributing private databases or resume
capabilities.

## Performance history and remaining scope

No new 0.9 server-capacity, storage-performance, RSS-plateau or GPU benchmark is
claimed. The [0.8 regional probe](regional-probe-v0.8.json),
[0.8 TLS session stress](regional-session-stress-tls-v0.8.json) and
[measurement provenance](regional-measurement-provenance-v0.8.json) retain their
original workloads, binaries and timings. They must not be relabeled as 0.9
measurements or combined with this release's correctness totals.

The [product review](PRODUCT_REVIEW.md) records resolved findings and remaining
acceptance priorities. [MOTORTOWN_PARITY.md](MOTORTOWN_PARITY.md) and
[MMORPG_FOUNDATION.md](MMORPG_FOUNDATION.md) retain explicit limits for production
physics/assets, public identity/operations, distributed authority, long-duration
balance, accessibility/hardware qualification and global historical data.
Passing bounded fixtures does not settle those requirements.
